//! The history on disk, so it survives a restart — and an update, which is a
//! restart the user did not ask for.
//!
//! Two pieces, on purpose. `index.json` holds everything small (text, source app,
//! timestamps) and is rewritten whole on every change. Image payloads are files
//! under `img/`, written once and referenced by id: a screenshot is hundreds of
//! kilobytes, and rewriting fifty of them every time someone copies a word would
//! be absurd. Saving is best-effort — a broken disk must not take the clipboard
//! down with it, so every failure is logged and swallowed.

use crate::history::{ClipItem, Payload, SourceApp};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const INDEX: &str = "index.json";
const IMG_DIR: &str = "img";

pub struct Store {
    dir: PathBuf,
}

/// One clip as it sits in `index.json`. Image bytes are not here — only the fact
/// that the payload is an image, and how big it was.
#[derive(Serialize, Deserialize)]
struct StoredClip {
    id: u64,
    created_at: u64,
    app_name: String,
    app_bundle: String,
    app_icon: String,
    /// "text" | "image"
    kind: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
}

impl Store {
    /// `dir` is the app's data directory; the history lives in a subfolder of it.
    pub fn new(dir: PathBuf) -> Self {
        let store = Store { dir };
        if let Err(e) = std::fs::create_dir_all(store.img_dir()) {
            crate::debug_log::log(&format!("store: cannot create {}: {}", store.img_dir().display(), e));
        }
        store
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join(INDEX)
    }

    fn img_dir(&self) -> PathBuf {
        self.dir.join(IMG_DIR)
    }

    fn img_path(&self, id: u64) -> PathBuf {
        self.img_dir().join(format!("{}.png", id))
    }

    /// Everything we saved last time, newest first, exactly as it was.
    pub fn load(&self) -> Vec<ClipItem> {
        let raw = match std::fs::read(self.index_path()) {
            Ok(r) => r,
            // No index yet is the normal first launch, not a failure.
            Err(_) => return Vec::new(),
        };
        let stored: Vec<StoredClip> = match serde_json::from_slice(&raw) {
            Ok(s) => s,
            Err(e) => {
                crate::debug_log::log(&format!("store: index unreadable, starting empty: {}", e));
                return Vec::new();
            }
        };

        let items: Vec<ClipItem> = stored
            .into_iter()
            .filter_map(|s| {
                let payload = match s.kind.as_str() {
                    "text" => Payload::Text(s.text),
                    "image" => {
                        // A missing or unreadable file means the clip is gone, not
                        // that the history is broken: drop the card, keep the rest.
                        let png = std::fs::read(self.img_path(s.id)).ok()?;
                        Payload::Image { png, width: s.width, height: s.height }
                    }
                    other => {
                        crate::debug_log::log(&format!("store: unknown kind {:?}, clip dropped", other));
                        return None;
                    }
                };
                Some(ClipItem {
                    id: s.id,
                    payload,
                    app: SourceApp { name: s.app_name, bundle: s.app_bundle, icon: s.app_icon },
                    created_at: s.created_at,
                })
            })
            .collect();

        crate::debug_log::log(&format!("store: {} clips restored", items.len()));
        items
    }

    /// Writes the whole history. Image files are written once (their content is
    /// keyed by id and never changes) and files no clip refers to any more —
    /// pushed off the end of the ring — are deleted.
    pub fn save(&self, items: &[ClipItem]) {
        for item in items {
            if let Payload::Image { png, .. } = &item.payload {
                let path = self.img_path(item.id);
                if !path.exists() {
                    if let Err(e) = std::fs::write(&path, png) {
                        crate::debug_log::log(&format!("store: cannot write {}: {}", path.display(), e));
                    }
                }
            }
        }

        let stored: Vec<StoredClip> = items
            .iter()
            .map(|item| {
                let (kind, text, width, height) = match &item.payload {
                    Payload::Text(s) => ("text", s.clone(), 0, 0),
                    Payload::Image { width, height, .. } => ("image", String::new(), *width, *height),
                };
                StoredClip {
                    id: item.id,
                    created_at: item.created_at,
                    app_name: item.app.name.clone(),
                    app_bundle: item.app.bundle.clone(),
                    app_icon: item.app.icon.clone(),
                    kind: kind.into(),
                    text,
                    width,
                    height,
                }
            })
            .collect();

        match serde_json::to_vec(&stored) {
            Ok(bytes) => {
                if let Err(e) = write_atomic(&self.index_path(), &bytes) {
                    crate::debug_log::log(&format!("store: cannot write index: {}", e));
                }
            }
            Err(e) => crate::debug_log::log(&format!("store: cannot encode index: {}", e)),
        }

        self.drop_orphan_images(items);
    }

    fn drop_orphan_images(&self, items: &[ClipItem]) {
        // Only image clips keep a file alive. Matching on the id alone would let a
        // text clip that happens to carry the same id shield a picture nobody can
        // reach any more.
        let live: std::collections::HashSet<String> = items
            .iter()
            .filter(|i| matches!(i.payload, Payload::Image { .. }))
            .map(|i| format!("{}.png", i.id))
            .collect();
        let entries = match std::fs::read_dir(self.img_dir()) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !live.contains(&name) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// A half-written index is worse than a stale one: the app reads it on the next
/// launch and finds garbage. Write beside it, then rename over — rename is atomic.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::History;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// A scratch directory of its own per test — the tests run in parallel.
    fn temp_store() -> Store {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("copypaster-test-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&dir);
        Store::new(dir)
    }

    fn app() -> SourceApp {
        SourceApp { name: "Ghostty".into(), bundle: "com.mitchellh.ghostty".into(), icon: String::new() }
    }

    fn png() -> Vec<u8> {
        // Not a real PNG — the store copies the bytes, it never decodes them.
        vec![0x89, 0x50, 0x4e, 0x47, 1, 2, 3]
    }

    #[test]
    fn a_restart_keeps_the_clips() {
        let store = temp_store();
        let mut h = History::new();
        h.add(Payload::Text("hello".into()), app(), 10);
        h.add(Payload::Image { png: png(), width: 800, height: 600 }, app(), 20);
        store.save(h.items());

        let mut restored = History::new();
        restored.restore(store.load());
        let v = restored.view();
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].text, "hello");
        assert_eq!(v[0].kind, "image");
        assert_eq!((v[0].width, v[0].height), (800, 600));
    }

    #[test]
    fn a_restored_image_still_pastes_its_own_bytes() {
        let store = temp_store();
        let mut h = History::new();
        h.add(Payload::Image { png: png(), width: 4, height: 4 }, app(), 1);
        store.save(h.items());

        let mut restored = History::new();
        restored.restore(store.load());
        let id = restored.view()[0].id;
        match &restored.get(id).unwrap().payload {
            Payload::Image { png: bytes, .. } => assert_eq!(bytes, &png()),
            _ => panic!("expected the image back"),
        }
    }

    #[test]
    fn a_fresh_clip_never_reuses_a_restored_id() {
        let store = temp_store();
        let mut h = History::new();
        h.add(Payload::Text("one".into()), app(), 1);
        h.add(Payload::Text("two".into()), app(), 2);
        store.save(h.items());

        let mut restored = History::new();
        restored.restore(store.load());
        restored.add(Payload::Text("three".into()), app(), 3);
        let ids: Vec<u64> = restored.view().iter().map(|c| c.id).collect();
        let unique: std::collections::HashSet<u64> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "ids collided: {:?}", ids);
    }

    #[test]
    fn an_image_that_fell_off_the_ring_leaves_no_file_behind() {
        let store = temp_store();
        let mut h = History::new();
        h.add(Payload::Image { png: png(), width: 1, height: 1 }, app(), 1);
        store.save(h.items());
        let orphan = store.img_path(h.view()[0].id);
        assert!(orphan.exists());

        // Push it out of the ring, then save again.
        let mut later = History::new();
        later.add(Payload::Text("newer".into()), app(), 2);
        store.save(later.items());
        assert!(!orphan.exists(), "the image file outlived its clip");
    }

    #[test]
    fn a_corrupt_index_starts_empty_instead_of_crashing() {
        let store = temp_store();
        std::fs::write(store.index_path(), b"{not json").unwrap();
        assert!(store.load().is_empty());
    }
}
