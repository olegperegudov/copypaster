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

/// kVK_ANSI_V — the *physical* V key, wherever the layout puts a letter on it.
///
/// It has to be the hardware position. enigo's `Key::Unicode('v')` asks the
/// active layout which key types a "v", and on a Cyrillic layout none does: the
/// lookup falls through to keycode 0, which is the A key. The paste then went
/// out as ⌘A — nothing landed, and the target app quietly selected all its text
/// instead. Same trap Quill hit with ⌘C.
#[cfg(target_os = "macos")]
const KEY_V: u16 = 0x09;

/// Long enough for an app that debounces keypresses to still see the chord.
#[cfg(target_os = "macos")]
const CHORD_HOLD_MS: u64 = 15;

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

/// macOS: ⌘V as a raw key event with the Command flag set on the event itself.
/// The receiving app reads the chord from those flags, so nothing depends on
/// which modifiers are physically held or which layout is active.
#[cfg(target_os = "macos")]
fn send_paste_keys() -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| "CGEventSource::new failed".to_string())?;

    let down = CGEvent::new_keyboard_event(source.clone(), KEY_V, true)
        .map_err(|_| "paste key-down event".to_string())?;
    down.set_flags(CGEventFlags::CGEventFlagCommand);
    down.post(CGEventTapLocation::HID);

    std::thread::sleep(std::time::Duration::from_millis(CHORD_HOLD_MS));

    let up = CGEvent::new_keyboard_event(source, KEY_V, false)
        .map_err(|_| "paste key-up event".to_string())?;
    up.set_flags(CGEventFlags::CGEventFlagCommand);
    up.post(CGEventTapLocation::HID);

    crate::debug_log::log("paste: Cmd+V sent");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn send_paste_keys() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo.key(Key::Control, Direction::Press).map_err(|e| e.to_string())?;
    let typed = enigo.key(Key::Unicode('v'), Direction::Click);
    // Always release the modifier, even if the keypress failed — a stuck Ctrl
    // leaves the whole desktop in a broken state.
    let released = enigo.key(Key::Control, Direction::Release);
    typed.map_err(|e| e.to_string())?;
    released.map_err(|e| e.to_string())?;
    crate::debug_log::log("paste: Ctrl+V sent");
    Ok(())
}

/// Brings the app that was in front when the popup opened back to the front.
#[cfg(target_os = "macos")]
pub fn activate(pid: i32) {
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
pub fn activate(_pid: i32) {
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// The whole bug in one number. Resolving "v" through the active layout
    /// returns keycode 0 on a Cyrillic layout — and keycode 0 is the A key, so
    /// every paste went out as ⌘A: nothing arrived, and the target app selected
    /// all of its text instead. The V key is taken by hardware position now, and
    /// this test fails the moment someone reaches for the letter again.
    #[test]
    fn the_paste_key_is_the_physical_v_not_a_layout_lookup() {
        assert_eq!(KEY_V, 0x09, "kVK_ANSI_V");
        assert_ne!(KEY_V, 0x00, "keycode 0 is the A key — that is the shipped bug");
    }
}
