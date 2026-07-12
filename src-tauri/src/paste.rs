//! Puts the picked clip on the clipboard and pastes it into the app the user
//! came from.
//!
//! Order matters: the popup is a non-activating panel, so the previous app never
//! lost focus — but its window is behind ours. Raise it first, give it a beat to
//! become key, then post the paste. Sending the keystroke while the panel is
//! still up types into nothing.

use crate::history::Payload;

/// Time for the target app to come forward before the keystroke lands.
const FOCUS_SETTLE_MS: u64 = 60;

pub fn paste(payload: &Payload, skip_next: &std::sync::Mutex<bool>, target_pid: Option<i32>) -> Result<(), String> {
    // Our own clipboard write must not come back as a fresh clip.
    if let Ok(mut skip) = skip_next.lock() {
        *skip = true;
    }
    if let Err(e) = crate::clipboard::write_clipboard(payload) {
        if let Ok(mut skip) = skip_next.lock() {
            *skip = false;
        }
        return Err(e);
    }

    if let Some(pid) = target_pid {
        activate(pid);
    }
    std::thread::sleep(std::time::Duration::from_millis(FOCUS_SETTLE_MS));
    send_paste_keys()
}

fn send_paste_keys() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo.key(modifier, Direction::Press).map_err(|e| e.to_string())?;
    let typed = enigo.key(Key::Unicode('v'), Direction::Click);
    // Always release the modifier, even if the keypress failed — a stuck Cmd
    // leaves the whole desktop in a broken state.
    let released = enigo.key(modifier, Direction::Release);
    typed.map_err(|e| e.to_string())?;
    released.map_err(|e| e.to_string())?;
    crate::debug_log::log("paste: Cmd+V sent");
    Ok(())
}

/// Brings the app that was in front when the popup opened back to the front.
#[cfg(target_os = "macos")]
fn activate(pid: i32) {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let app: id = msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: pid];
        if app == nil {
            crate::debug_log::log(&format!("paste: pid {} is gone", pid));
            return;
        }
        // NSApplicationActivateIgnoringOtherApps
        let _: bool = msg_send![app, activateWithOptions: 1u64];
    }
}

#[cfg(not(target_os = "macos"))]
fn activate(_pid: i32) {
    // Windows never took focus away — the popup is a skip-taskbar tool window.
}

/// The app in front right now, remembered when the popup opens so the paste can
/// go back to it.
#[cfg(target_os = "macos")]
pub fn frontmost_pid() -> Option<i32> {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace == nil {
            return None;
        }
        let app: id = msg_send![workspace, frontmostApplication];
        if app == nil {
            return None;
        }
        let pid: i32 = msg_send![app, processIdentifier];
        Some(pid)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_pid() -> Option<i32> {
    None
}
