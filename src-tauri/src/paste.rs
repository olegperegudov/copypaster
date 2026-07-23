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

/// Shown when the paste is refused, and written to the log. The user reads the
/// system prompt, not this — it is here so a bug report says why nothing landed.
pub const NEEDS_ACCESSIBILITY: &str =
    "Iago has no Accessibility grant, so the keystroke would be dropped";

/// The gate in front of the keystroke, kept apart from the syscall so the
/// refusal has a test. Without the grant macOS eats the event and returns no
/// error, so posting it anyway would report a paste that never happened.
fn gate(trusted: bool) -> Result<(), String> {
    if trusted {
        Ok(())
    } else {
        Err(NEEDS_ACCESSIBILITY.to_string())
    }
}

/// Asks macOS whether this app may post keyboard events, and — with `prompt` —
/// puts up the system dialog that offers to open the settings pane.
///
/// The call is also what *lists* the app under Privacy & Security →
/// Accessibility: an app that never asks never appears there, so the user has
/// nothing to switch on and every paste dies silently. That was the bug behind
/// the rename: the grant belonged to the old bundle identifier, the new one had
/// never introduced itself, and the pane showed no Iago at all.
#[cfg(target_os = "macos")]
pub fn accessibility_trusted(prompt: bool) -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let options = CFDictionary::from_CFType_pairs(&[(
            key.as_CFType(),
            CFBoolean::from(prompt).as_CFType(),
        )]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) != 0
    }
}

#[cfg(not(target_os = "macos"))]
pub fn accessibility_trusted(_prompt: bool) -> bool {
    true
}

/// macOS: ⌘V as a raw key event with the Command flag set on the event itself.
/// The receiving app reads the chord from those flags, so nothing depends on
/// which modifiers are physically held or which layout is active.
#[cfg(target_os = "macos")]
fn send_paste_keys() -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    // Ask before posting: without the grant the event is dropped in silence, and
    // asking is what raises the prompt and lists the app in the settings pane.
    if let Err(e) = gate(accessibility_trusted(false)) {
        accessibility_trusted(true);
        crate::debug_log::log("paste: refused — no Accessibility grant, prompted for it");
        return Err(e);
    }

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

    crate::debug_log::log("paste: Cmd+V posted, grant in place");
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

    /// macOS drops a posted keystroke from an untrusted app without an error, so
    /// the only honest thing to do without the grant is refuse. Posting anyway
    /// is what made the app report a paste it never delivered.
    #[test]
    fn without_the_accessibility_grant_the_paste_is_refused_not_reported_as_sent() {
        assert!(gate(false).is_err(), "no grant must not read as a delivered paste");
        assert!(gate(true).is_ok());
    }
}
