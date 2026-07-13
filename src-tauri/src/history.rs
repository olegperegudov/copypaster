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

    /// Adds a clip at the head. Returns false when it was a duplicate of the
    /// current head — the same content copied twice in a row is one clip, not
    /// two, and re-copying the clip we just pasted must not grow the history.
    pub fn add(&mut self, payload: Payload, app: SourceApp, created_at: u64) -> bool {
        if let Some(head) = self.items.first() {
            if same_content(&head.payload, &payload) {
                return false;
            }
        }
        let id = self.next_id;
        self.next_id += 1;
        self.items.insert(0, ClipItem { id, payload, app, created_at });
        self.items.truncate(MAX_ITEMS);
        true
    }

    /// Puts back what was on disk at launch. Ids continue from the highest one
    /// seen: a fresh clip must never reuse the id of a restored one, or picking a
    /// card would hand back the wrong content.
    pub fn restore(&mut self, items: Vec<ClipItem>) {
        self.next_id = items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        self.items = items;
        self.items.truncate(MAX_ITEMS);
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
        assert!(!h.add(Payload::Text("dup".into()), app(), 2));
        assert_eq!(h.view().len(), 1);
    }

    #[test]
    fn same_text_returns_after_something_else() {
        let mut h = History::new();
        h.add(Payload::Text("a".into()), app(), 1);
        h.add(Payload::Text("b".into()), app(), 2);
        assert!(h.add(Payload::Text("a".into()), app(), 3));
        assert_eq!(h.view().len(), 3);
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
