//! Watches the system clipboard and feeds new clips into the history.
//!
//! Change *detection* is platform-native and cheap (a counter the OS bumps on
//! every write); only when that counter moves do we actually read the content.
//! Polling `arboard` blindly would decode every image on the clipboard twice a
//! second — for a 4K screenshot that is ~30 MB of pointless work per tick.

use crate::history::{now_secs, History, Payload};
use crate::source_app;
use std::sync::{Arc, Mutex};

/// How often we ask the OS "did the clipboard change". Cheap: one integer read.
const POLL_MS: u64 = 300;

pub struct Watcher {
    history: Arc<Mutex<History>>,
    /// Set before we write to the clipboard ourselves (paste, screenshot copy).
    /// Without it our own write bounces back through this watcher and lands in
    /// the history a second time.
    pub skip_next: Arc<Mutex<bool>>,
    last_change: u64,
}

impl Watcher {
    pub fn new(history: Arc<Mutex<History>>, skip_next: Arc<Mutex<bool>>) -> Self {
        Watcher { history, skip_next, last_change: change_count(), }
    }

    /// Runs forever on its own thread; returns true from `tick` when the history
    /// grew, so the caller can tell the popup to redraw.
    pub fn run<F: Fn()>(mut self, on_change: F) {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
            if self.tick() {
                on_change();
            }
        }
    }

    fn tick(&mut self) -> bool {
        let cc = change_count();
        if cc == self.last_change {
            return false;
        }
        self.last_change = cc;

        if let Ok(mut skip) = self.skip_next.lock() {
            if *skip {
                *skip = false;
                crate::debug_log::log("clipboard: change swallowed (our own write)");
                return false;
            }
        }

        // A password manager marks what it puts on the clipboard as concealed.
        // Read it and we would file the user's password into a plaintext history
        // that outlives the 30 seconds the manager keeps it around for.
        if is_concealed() {
            crate::debug_log::log("clipboard: concealed clip ignored (password manager)");
            return false;
        }

        let payload = match read_clipboard() {
            Some(p) => p,
            None => return false,
        };
        let app = source_app::frontmost();
        match &payload {
            Payload::Text(s) => crate::debug_log::log(&format!(
                "clipboard: text ({} chars) from {}",
                s.chars().count(),
                app.name
            )),
            Payload::Image { width, height, .. } => crate::debug_log::log(&format!(
                "clipboard: image {}x{} from {}",
                width, height, app.name
            )),
        }
        match self.history.lock() {
            Ok(mut h) => h.add(payload, app, now_secs()),
            Err(_) => false,
        }
    }
}

/// Reads whatever is on the clipboard now. Image wins over text: an image copied
/// from a browser also carries its alt text / URL, and the user copied the
/// picture.
fn read_clipboard() -> Option<Payload> {
    let mut cb = arboard::Clipboard::new().ok()?;
    if let Ok(img) = cb.get_image() {
        let width = img.width as u32;
        let height = img.height as u32;
        let buf = image::RgbaImage::from_raw(width, height, img.bytes.into_owned())?;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buf)
            .write_to(&mut png, image::ImageFormat::Png)
            .ok()?;
        return Some(Payload::Image { png: png.into_inner(), width, height });
    }
    let text = cb.get_text().ok()?;
    if text.trim().is_empty() {
        return None;
    }
    Some(Payload::Text(text))
}

/// Puts a clip back on the clipboard. The caller must raise `skip_next` first.
pub fn write_clipboard(payload: &Payload) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    match payload {
        Payload::Text(s) => cb.set_text(s.clone()).map_err(|e| e.to_string()),
        Payload::Image { png, .. } => {
            let img = image::load_from_memory(png).map_err(|e| e.to_string())?.to_rgba8();
            let (w, h) = img.dimensions();
            cb.set_image(arboard::ImageData {
                width: w as usize,
                height: h as usize,
                bytes: std::borrow::Cow::Owned(img.into_raw()),
            })
            .map_err(|e| e.to_string())
        }
    }
}

/// The OS's "clipboard generation" counter.
#[cfg(target_os = "macos")]
fn change_count() -> u64 {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pb: id = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb == nil {
            return 0;
        }
        let cc: i64 = msg_send![pb, changeCount];
        cc as u64
    }
}

#[cfg(target_os = "windows")]
fn change_count() -> u64 {
    unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() as u64 }
}

/// Types that mean "clipboard managers: not this one". The convention is
/// nspasteboard.org's, and every password manager on macOS follows it —
/// 1Password, Keychain, Bitwarden all stamp a copied password `ConcealedType`.
/// Transient and auto-generated clips are somebody's intermediate step, not
/// something the user copied on purpose.
#[cfg(target_os = "macos")]
const IGNORED_TYPES: [&str; 3] = [
    "org.nspasteboard.ConcealedType",
    "org.nspasteboard.TransientType",
    "org.nspasteboard.AutoGeneratedType",
];

/// True when the current clip is marked as one nobody should record.
#[cfg(target_os = "macos")]
fn is_concealed() -> bool {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pb: id = msg_send![class!(NSPasteboard), generalPasteboard];
        if pb == nil {
            return false;
        }
        let types: id = msg_send![pb, types];
        if types == nil {
            return false;
        }
        let count: usize = msg_send![types, count];
        for i in 0..count {
            let t: id = msg_send![types, objectAtIndex: i];
            let utf8: *const std::os::raw::c_char = msg_send![t, UTF8String];
            if utf8.is_null() {
                continue;
            }
            let name = std::ffi::CStr::from_ptr(utf8).to_string_lossy();
            if IGNORED_TYPES.contains(&name.as_ref()) {
                return true;
            }
        }
        false
    }
}

/// Windows has its own marker: a password manager registers the
/// `ExcludeClipboardContentFromMonitorProcessing` format, which is exactly the
/// "do not put this in a history" flag Windows' own clipboard history honours.
#[cfg(target_os = "windows")]
fn is_concealed() -> bool {
    use windows::core::w;
    use windows::Win32::System::DataExchange::{IsClipboardFormatAvailable, RegisterClipboardFormatW};
    unsafe {
        let fmt = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        fmt != 0 && IsClipboardFormatAvailable(fmt).is_ok()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn is_concealed() -> bool {
    false
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    /// Reads the real pasteboard, so it cannot run beside the other tests (they
    /// share one machine and one clipboard) and it needs a clip staged first:
    ///
    ///   swift tests/stage_concealed_clip.swift   # then:
    ///   cargo test --lib -- --ignored concealed
    ///
    /// Kept because the whole password protection hangs on this one call: if
    /// `types` ever stops carrying the marker, the history silently starts
    /// eating passwords again, and no other test would notice.
    #[test]
    #[ignore]
    fn a_concealed_clip_is_recognised() {
        assert!(super::is_concealed(), "staged a concealed clip and the watcher did not see it");
    }

    #[test]
    #[ignore]
    fn an_ordinary_clip_is_not_concealed() {
        assert!(!super::is_concealed(), "an ordinary clip must still reach the history");
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn change_count() -> u64 {
    0
}
