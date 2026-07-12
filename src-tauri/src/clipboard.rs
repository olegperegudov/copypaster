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

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn change_count() -> u64 {
    0
}
