//! Which app the clip came from.
//!
//! Read at the moment the clipboard changes, not when the popup opens — by then
//! the frontmost app is CopyPaster itself. The name is the card header, the
//! bundle id is the filter key, the icon is cached per bundle id because
//! rasterising an NSImage on every copy is pure waste.

use crate::history::SourceApp;
use std::collections::HashMap;
use std::sync::Mutex;

static ICON_CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// The app in front right now, with its icon as a PNG data-URL.
pub fn frontmost() -> SourceApp {
    let mut app = frontmost_raw();
    if app.bundle.is_empty() {
        return app;
    }

    let mut guard = match ICON_CACHE.lock() {
        Ok(g) => g,
        Err(_) => return app,
    };
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(cached) = cache.get(&app.bundle) {
        app.icon = cached.clone();
        return app;
    }
    let icon = icon_data_url(&app.bundle);
    cache.insert(app.bundle.clone(), icon.clone());
    app.icon = icon;
    app
}

/// Longest side of a cached app icon. The card header draws it at 16 px.
#[cfg(target_os = "macos")]
const ICON_SIDE: u32 = 64;

#[cfg(target_os = "macos")]
fn frontmost_raw() -> SourceApp {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace == nil {
            return SourceApp::default();
        }
        let running: id = msg_send![workspace, frontmostApplication];
        if running == nil {
            return SourceApp::default();
        }
        let name: id = msg_send![running, localizedName];
        let bundle: id = msg_send![running, bundleIdentifier];
        SourceApp {
            name: nsstring_to_string(name),
            bundle: nsstring_to_string(bundle),
            icon: String::new(),
        }
    }
}

/// Rasterises the app's icon to a downscaled PNG data-URL.
///
/// AppKit does the encoding, not the `image` crate. An NSImage's TIFF
/// representation can be 16-bit float per channel (Ghostty's and Telegram's
/// are), which `image` refuses to decode — the icons came back blank. Asking
/// NSBitmapImageRep for a PNG hands us plain 8-bit RGBA that anything can read.
#[cfg(target_os = "macos")]
fn icon_data_url(bundle: &str) -> String {
    use base64::Engine;
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSData, NSString};
    use objc::{class, msg_send, sel, sel_impl};

    /// NSBitmapImageFileTypePNG
    const PNG_TYPE: u64 = 4;

    let png: Vec<u8> = unsafe {
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace == nil {
            return String::new();
        }
        let ns_bundle = NSString::alloc(nil).init_str(bundle);
        let path: id = msg_send![workspace, absolutePathForAppBundleWithIdentifier: ns_bundle];
        if path == nil {
            return String::new();
        }
        let icon: id = msg_send![workspace, iconForFile: path];
        if icon == nil {
            return String::new();
        }
        let tiff: id = msg_send![icon, TIFFRepresentation];
        if tiff == nil {
            return String::new();
        }
        let rep: id = msg_send![class!(NSBitmapImageRep), imageRepWithData: tiff];
        if rep == nil {
            return String::new();
        }
        let props: id = msg_send![class!(NSDictionary), dictionary];
        let data: id = msg_send![rep, representationUsingType: PNG_TYPE properties: props];
        if data == nil {
            return String::new();
        }
        let bytes = data.bytes() as *const u8;
        let len = data.length() as usize;
        if bytes.is_null() || len == 0 {
            return String::new();
        }
        std::slice::from_raw_parts(bytes, len).to_vec()
    };

    let img = match image::load_from_memory(&png) {
        Ok(i) => i,
        Err(e) => {
            crate::debug_log::log(&format!("icon: decode failed for {}: {}", bundle, e));
            return String::new();
        }
    };
    let small = img.thumbnail(ICON_SIDE, ICON_SIDE);
    let mut buf = std::io::Cursor::new(Vec::new());
    if small.write_to(&mut buf, image::ImageFormat::Png).is_err() {
        return String::new();
    }
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
    )
}

#[cfg(target_os = "macos")]
unsafe fn nsstring_to_string(s: cocoa::base::id) -> String {
    use cocoa::base::nil;
    use cocoa::foundation::NSString;
    if s == nil {
        return String::new();
    }
    let bytes = s.UTF8String() as *const std::os::raw::c_char;
    if bytes.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(bytes).to_string_lossy().into_owned()
}

// Windows: the card renders without a source header until this is filled in.
// Deliberately empty rather than guessed — a wrong app name in the header is
// worse than none, because the app row filters on it.
#[cfg(not(target_os = "macos"))]
fn frontmost_raw() -> SourceApp {
    SourceApp::default()
}

#[cfg(not(target_os = "macos"))]
fn icon_data_url(_bundle: &str) -> String {
    String::new()
}

/// The name shown on a screenshot card. Screenshots have no source app — they
/// come from the system, and grouping them under whatever window happened to be
/// in front would put them in the wrong filter bucket.
pub fn screenshot_source() -> SourceApp {
    SourceApp {
        name: "Screenshot".into(),
        bundle: "system.screenshot".into(),
        icon: String::new(),
    }
}
