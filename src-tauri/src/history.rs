//! Clipboard history: the items and the ring buffer that holds them.
//!
//! An item carries two representations of its content on purpose. The *payload*
//! (full text, or the PNG bytes of an image) is what goes back on the clipboard
//! when the user picks the card. The *preview* (trimmed text, downscaled PNG
//! data-URL) is what the webview renders — handing a 4K screenshot to the UI as
//! a data-URL once per card would blow the popup's open time for no gain.

use serde::Serialize;

/// Cards the popup can show at once. Beyond this the oldest clip falls off.
const MAX_ITEMS: usize = 50;
/// Longest side of a card preview image, in pixels.
const PREVIEW_MAX_SIDE: u32 = 320;
/// Preview text handed to the UI; a card shows ~7 lines, the rest is dead weight.
const PREVIEW_CHARS: usize = 400;

#[derive(Clone)]
pub enum Payload {
    Text(String),
    /// PNG bytes. Kept encoded, not as raw RGBA: a 4K screenshot is ~30 MB raw
    /// and a few hundred KB as PNG, and the history holds up to 50 of them.
    Image { png: Vec<u8>, width: u32, height: u32 },
}

#[derive(Clone)]
pub struct ClipItem {
    pub id: u64,
    pub payload: Payload,
    pub app: SourceApp,
    /// Unix epoch, seconds. The UI turns this into "4 min".
    pub created_at: u64,
    /// Hash of the payload, kept so a re-copy can be recognised without comparing
    /// its bytes against fifty clips — an image payload is hundreds of kilobytes.
    /// Not persisted: it is derived from the payload and recomputed on load.
    hash: u64,
}

impl ClipItem {
    pub fn new(id: u64, payload: Payload, app: SourceApp, created_at: u64) -> Self {
        let hash = content_hash(&payload);
        ClipItem { id, payload, app, created_at, hash }
    }
}

/// The app the clip was copied from — the card's header, and what the app row
/// filters on. `bundle` is the stable key (icons are cached by it); `name` is
/// what the user reads.
#[derive(Clone, Default, PartialEq)]
pub struct SourceApp {
    pub name: String,
    pub bundle: String,
    /// PNG data-URL of the app icon, or empty when we could not read one.
    pub icon: String,
}

/// The shape handed to the webview. Payload-independent so the frontend never
/// branches on a Rust enum it cannot see.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipView {
    pub id: u64,
    /// "text" | "image"
    pub kind: &'static str,
    /// Full text for a text clip (trimmed to PREVIEW_CHARS), empty for an image.
    /// The search matches on this, so it must be the real content, not a label.
    pub text: String,
    /// data:image/png;base64,… of the downscaled preview, empty for a text clip.
    pub preview: String,
    pub width: u32,
    pub height: u32,
    /// Character count of the *full* text, before trimming — the card's footer.
    pub chars: usize,
    pub app_name: String,
    pub app_bundle: String,
    pub app_icon: String,
    pub created_at: u64,
}

pub struct History {
    items: Vec<ClipItem>,
    next_id: u64,
}

impl History {
    pub fn new() -> Self {
        History { items: Vec::new(), next_id: 1 }
    }

    /// Files a clip. Content the history already holds is not a second card: the
    /// clip we have moves to the head and takes the new time and source app —
    /// copying the same snippet ten times a day should leave one card that says
    /// "just now", not ten identical ones pushing everything else off the ring.
    ///
    /// Returns whether anything actually changed, so an unchanged history costs
    /// neither a disk write nor a redraw.
    pub fn add(&mut self, payload: Payload, app: SourceApp, created_at: u64) -> bool {
        let hash = content_hash(&payload);
        // The hash narrows it down; the byte compare settles it. A collision must
        // not hand the user someone else's clip when they press the card.
        let twin = self
            .items
            .iter()
            .position(|i| i.hash == hash && same_content(&i.payload, &payload));

        if let Some(pos) = twin {
            let unchanged = pos == 0 && self.items[0].created_at == created_at;
            let mut item = self.items.remove(pos);
            item.created_at = created_at;
            // Where it came from is where it was *last* copied from — the header
            // and the timestamp on one card have to tell the same story. An app we
            // could not identify does not get to erase the one we knew.
            if !app.bundle.is_empty() {
                item.app = app;
            }
            self.items.insert(0, item);
            return !unchanged;
        }

        let id = self.next_id;
        self.next_id += 1;
        self.items.insert(0, ClipItem::new(id, payload, app, created_at));
        self.items.truncate(MAX_ITEMS);
        true
    }

    /// Puts back what was on disk at launch. Ids continue from the highest one
    /// seen: a fresh clip must never reuse the id of a restored one, or picking a
    /// card would hand back the wrong content.
    ///
    /// Duplicates are collapsed on the way in — the file was written by a version
    /// that let the same content sit on several cards, and the user should not
    /// have to copy each one again to be rid of it. Newest first means the copy we
    /// keep is the most recent one. Returns how many it collapsed, so the caller
    /// can write the cleaned history back instead of leaving the copies on disk.
    pub fn restore(&mut self, items: Vec<ClipItem>) -> usize {
        self.next_id = items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        let before = items.len();
        let mut kept: Vec<ClipItem> = Vec::with_capacity(items.len());
        for item in items {
            let dup = kept
                .iter()
                .any(|k| k.hash == item.hash && same_content(&k.payload, &item.payload));
            if !dup {
                kept.push(item);
            }
        }
        let collapsed = before - kept.len();
        self.items = kept;
        self.items.truncate(MAX_ITEMS);
        collapsed
    }

    /// Drops one clip for good. Returns false when the id is already gone: the
    /// card the user pressed on can be a beat behind the history, and a stale
    /// press must not take a neighbour with it.
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|i| i.id != id);
        self.items.len() != before
    }

    /// Drops clips older than the retention the user chose. `None` = they chose
    /// no expiry. Returns how many went, so the caller knows whether the index on
    /// disk needs rewriting.
    ///
    /// The ring (50 cards) bounds *size*; this bounds *time*. Without it a clip
    /// nobody pushed out lives forever — and "everything I ever copied" is not
    /// what a person thinks they are keeping when they keep a clipboard history.
    pub fn prune_expired(&mut self, now: u64, max_age_secs: Option<u64>) -> usize {
        let Some(max_age) = max_age_secs else { return 0 };
        let before = self.items.len();
        self.items.retain(|i| now.saturating_sub(i.created_at) < max_age);
        before - self.items.len()
    }

    pub fn items(&self) -> &[ClipItem] {
        &self.items
    }

    pub fn get(&self, id: u64) -> Option<&ClipItem> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn view(&self) -> Vec<ClipView> {
        self.items.iter().map(to_view).collect()
    }
}

/// A cheap fingerprint of what the clip holds. Kind is hashed with it so a text
/// clip and an image can never look alike, whatever their bytes.
fn content_hash(payload: &Payload) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    match payload {
        Payload::Text(s) => {
            0u8.hash(&mut h);
            s.hash(&mut h);
        }
        Payload::Image { png, .. } => {
            1u8.hash(&mut h);
            png.hash(&mut h);
        }
    }
    h.finish()
}

fn same_content(a: &Payload, b: &Payload) -> bool {
    match (a, b) {
        (Payload::Text(x), Payload::Text(y)) => x == y,
        (Payload::Image { png: x, .. }, Payload::Image { png: y, .. }) => x == y,
        _ => false,
    }
}

fn to_view(item: &ClipItem) -> ClipView {
    let (kind, text, preview, width, height, chars) = match &item.payload {
        Payload::Text(s) => {
            let chars = s.chars().count();
            let trimmed: String = s.chars().take(PREVIEW_CHARS).collect();
            ("text", trimmed, String::new(), 0, 0, chars)
        }
        Payload::Image { png, width, height } => {
            ("image", String::new(), preview_data_url(png), *width, *height, 0)
        }
    };
    ClipView {
        id: item.id,
        kind,
        text,
        preview,
        width,
        height,
        chars,
        app_name: item.app.name.clone(),
        app_bundle: item.app.bundle.clone(),
        app_icon: item.app.icon.clone(),
        created_at: item.created_at,
    }
}

/// Downscales to PREVIEW_MAX_SIDE and re-encodes as a PNG data-URL. On any
/// decode/encode failure the card simply renders without an image rather than
/// taking the whole popup down with it.
fn preview_data_url(png: &[u8]) -> String {
    use base64::Engine;
    let img = match image::load_from_memory(png) {
        Ok(i) => i,
        Err(e) => {
            crate::debug_log::log(&format!("preview: decode failed: {}", e));
            return String::new();
        }
    };
    let small = img.thumbnail(PREVIEW_MAX_SIDE, PREVIEW_MAX_SIDE);
    let mut buf = std::io::Cursor::new(Vec::new());
    if let Err(e) = small.write_to(&mut buf, image::ImageFormat::Png) {
        crate::debug_log::log(&format!("preview: encode failed: {}", e));
        return String::new();
    }
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
    )
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> SourceApp {
        SourceApp { name: "Ghostty".into(), bundle: "com.mitchellh.ghostty".into(), icon: String::new() }
    }

    const DAY: u64 = 86_400;

    #[test]
    fn a_clip_older_than_the_retention_is_dropped() {
        let mut h = History::new();
        let now = 100 * DAY;
        h.add(Payload::Text("last month".into()), app(), now - 30 * DAY);
        h.add(Payload::Text("yesterday".into()), app(), now - DAY);

        let gone = h.prune_expired(now, Some(7 * DAY));
        assert_eq!(gone, 1);
        let left: Vec<String> = h.view().into_iter().map(|c| c.text).collect();
        assert_eq!(left, ["yesterday"], "the week-old cutoff kept the wrong clip");
    }

    #[test]
    fn no_expiry_keeps_everything() {
        let mut h = History::new();
        let now = 100 * DAY;
        h.add(Payload::Text("ancient".into()), app(), 0);
        assert_eq!(h.prune_expired(now, None), 0);
        assert_eq!(h.view().len(), 1);
    }

    #[test]
    fn a_clip_from_the_future_is_not_treated_as_ancient() {
        // A clock that moved back (timezone, NTP) must not wipe the history.
        let mut h = History::new();
        h.add(Payload::Text("fresh".into()), app(), 100 * DAY);
        assert_eq!(h.prune_expired(99 * DAY, Some(DAY)), 0);
    }

    #[test]
    fn a_deleted_clip_leaves_the_history() {
        let mut h = History::new();
        h.add(Payload::Text("keep".into()), app(), 1);
        h.add(Payload::Text("drop".into()), app(), 2);
        let doomed = h.view()[0].id;

        assert!(h.remove(doomed));
        let left = h.view();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].text, "keep");
    }

    #[test]
    fn deleting_a_clip_that_is_already_gone_takes_no_neighbour() {
        let mut h = History::new();
        h.add(Payload::Text("keep".into()), app(), 1);
        let id = h.view()[0].id;
        h.remove(id);

        // A stale card in the popup can press on an id the history no longer has.
        assert!(!h.remove(id));
        assert!(h.view().is_empty());
    }

    #[test]
    fn newest_clip_comes_first() {
        let mut h = History::new();
        h.add(Payload::Text("first".into()), app(), 1);
        h.add(Payload::Text("second".into()), app(), 2);
        let v = h.view();
        assert_eq!(v[0].text, "second");
        assert_eq!(v[1].text, "first");
    }

    #[test]
    fn copying_the_same_thing_twice_adds_one_clip() {
        let mut h = History::new();
        assert!(h.add(Payload::Text("dup".into()), app(), 1));
        h.add(Payload::Text("dup".into()), app(), 2);
        let v = h.view();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].created_at, 2, "the card still says it was copied a while ago");
    }

    #[test]
    fn re_copying_the_same_clip_at_the_same_moment_changes_nothing() {
        // The watcher can see one copy twice. Without this the app rewrites the
        // index and redraws the popup for a history that did not move.
        let mut h = History::new();
        h.add(Payload::Text("dup".into()), app(), 1);
        assert!(!h.add(Payload::Text("dup".into()), app(), 1));
    }

    #[test]
    fn copying_something_from_further_down_lifts_it_back_to_the_front() {
        let mut h = History::new();
        h.add(Payload::Text("a".into()), app(), 1);
        h.add(Payload::Text("b".into()), app(), 2);
        let old_id = h.view()[1].id;

        assert!(h.add(Payload::Text("a".into()), app(), 3));
        let v = h.view();
        assert_eq!(v.len(), 2, "the copy came back as a second card");
        assert_eq!(v[0].text, "a");
        assert_eq!(v[0].created_at, 3);
        assert_eq!(v[0].id, old_id, "the clip was rebuilt instead of moved — its image file would be orphaned");
    }

    #[test]
    fn the_same_image_copied_again_is_the_same_card() {
        let mut h = History::new();
        let png = vec![0x89, 0x50, 1, 2, 3];
        h.add(Payload::Image { png: png.clone(), width: 2, height: 2 }, app(), 1);
        h.add(Payload::Text("between".into()), app(), 2);
        h.add(Payload::Image { png, width: 2, height: 2 }, app(), 3);

        let v = h.view();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].kind, "image");
    }

    #[test]
    fn a_returning_clip_says_where_it_was_copied_from_this_time() {
        let mut h = History::new();
        h.add(Payload::Text("path".into()), app(), 1);
        let browser = SourceApp { name: "Safari".into(), bundle: "com.apple.Safari".into(), icon: String::new() };
        h.add(Payload::Text("path".into()), browser, 2);
        assert_eq!(h.view()[0].app_name, "Safari");
    }

    #[test]
    fn an_unknown_app_does_not_erase_the_one_we_knew() {
        let mut h = History::new();
        h.add(Payload::Text("path".into()), app(), 1);
        h.add(Payload::Text("path".into()), SourceApp::default(), 2);
        assert_eq!(h.view()[0].app_name, "Ghostty");
    }

    #[test]
    fn duplicates_written_by_an_older_version_are_collapsed_on_load() {
        let mut h = History::new();
        h.restore(vec![
            ClipItem::new(3, Payload::Text("dup".into()), app(), 30),
            ClipItem::new(2, Payload::Text("other".into()), app(), 20),
            ClipItem::new(1, Payload::Text("dup".into()), app(), 10),
        ]);
        let v = h.view();
        assert_eq!(v.len(), 2, "the old file's duplicate survived the load");
        assert_eq!(v[0].id, 3, "the copy we kept is not the most recent one");
        assert_eq!(v[1].text, "other");
    }

    #[test]
    fn history_is_capped() {
        let mut h = History::new();
        for i in 0..(MAX_ITEMS + 10) {
            h.add(Payload::Text(format!("clip {}", i)), app(), i as u64);
        }
        assert_eq!(h.view().len(), MAX_ITEMS);
    }

    #[test]
    fn text_preview_is_trimmed_but_char_count_is_full() {
        let mut h = History::new();
        let long = "a".repeat(PREVIEW_CHARS + 50);
        h.add(Payload::Text(long), app(), 1);
        let v = h.view();
        assert_eq!(v[0].text.chars().count(), PREVIEW_CHARS);
        assert_eq!(v[0].chars, PREVIEW_CHARS + 50);
    }

    #[test]
    fn picking_by_id_finds_the_clip() {
        let mut h = History::new();
        h.add(Payload::Text("one".into()), app(), 1);
        h.add(Payload::Text("two".into()), app(), 2);
        let id = h.view()[1].id;
        match &h.get(id).unwrap().payload {
            Payload::Text(s) => assert_eq!(s, "one"),
            _ => panic!("expected text"),
        }
    }
}
