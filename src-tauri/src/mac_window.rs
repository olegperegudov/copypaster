//! The popup is a non-activating NSPanel, not a window.
//!
//! This is the whole trick behind "picking a card pastes into the app you were
//! just in". A normal window steals focus when shown, so the app underneath is
//! no longer frontmost and the synthetic Cmd+V goes nowhere useful. A panel with
//! the NonactivatingPanel style mask takes keystrokes (we need typing in the
//! search field) without activating CopyPaster — the same mechanism Spotlight
//! and Raycast use. It also surfaces over another app's full-screen Space, which
//! a plain window cannot do.

#[cfg(target_os = "macos")]
use tauri::Manager as _;

#[cfg(target_os = "macos")]
tauri_nspanel::tauri_panel! {
    panel!(CopyPasterPanel {
        config: {
            can_become_key_window: true,   // the search field must accept typing
            can_become_main_window: false,
            is_floating_panel: true        // always over the app being pasted into
        }
    })
}

#[cfg(target_os = "macos")]
pub fn setup_panel(window: &tauri::WebviewWindow) -> Result<(), String> {
    use tauri_nspanel::{CollectionBehavior, StyleMask, WebviewWindowExt};

    let panel = window.to_panel::<CopyPasterPanel>().map_err(|e| e.to_string())?;
    panel.set_style_mask(StyleMask::empty().nonactivating_panel().into());
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .full_screen_auxiliary()
            .can_join_all_spaces()
            .into(),
    );
    // Stay up until the user picks or presses Esc — not dismissed just because
    // CopyPaster is not the active app (it never is, by design).
    panel.set_hides_on_deactivate(false);
    crate::debug_log::log("panel: popup converted to non-activating NSPanel");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn setup_panel(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn show_popup(app: &tauri::AppHandle) {
    use tauri_nspanel::ManagerExt;
    match app.get_webview_panel("main") {
        Ok(p) => p.show_and_make_key(),
        Err(e) => crate::debug_log::log(&format!("show_popup: panel missing ({:?})", e)),
    }
}

#[cfg(target_os = "macos")]
pub fn hide_popup(app: &tauri::AppHandle) {
    use tauri_nspanel::ManagerExt;
    if let Ok(p) = app.get_webview_panel("main") {
        p.hide();
    }
}

/// Dismisses the popup when the user clicks anywhere outside it.
///
/// A non-activating panel never becomes the active app, so it gets no "you lost
/// focus" callback to hang this on — clicking another window simply does not
/// concern us. A global NSEvent monitor does: it reports mouse-downs that landed
/// in *other* applications and never fires for clicks inside our own window, so
/// picking a card cannot dismiss the popup out from under itself. Mouse monitors
/// need no Accessibility grant (only keyboard ones do).
#[cfg(target_os = "macos")]
pub fn dismiss_on_outside_click(app: tauri::AppHandle) {
    use block::ConcreteBlock;
    use cocoa::base::id;
    use objc::{class, msg_send, sel, sel_impl};

    const LEFT_MOUSE_DOWN: u64 = 1 << 1;
    const RIGHT_MOUSE_DOWN: u64 = 1 << 3;
    const OTHER_MOUSE_DOWN: u64 = 1 << 25;

    let handler = ConcreteBlock::new(move |_event: id| {
        if popup_visible(&app) {
            hide_popup(&app);
        }
    });
    // The monitor outlives this call and keeps calling the block, so the block has
    // to outlive it too — copied to the heap and deliberately never freed.
    let handler = handler.copy();
    unsafe {
        let mask = LEFT_MOUSE_DOWN | RIGHT_MOUSE_DOWN | OTHER_MOUSE_DOWN;
        let _: id = msg_send![class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: mask
            handler: &*handler];
    }
    std::mem::forget(handler);
    crate::debug_log::log("panel: watching for clicks outside the popup");
}

// Windows needs no monitor: the popup there is an ordinary window, and clicking
// another one takes focus away from it — see the focus handler in lib.rs.
#[cfg(not(target_os = "macos"))]
pub fn dismiss_on_outside_click(_app: tauri::AppHandle) {}

#[cfg(target_os = "macos")]
pub fn popup_visible(app: &tauri::AppHandle) -> bool {
    use tauri_nspanel::ManagerExt;
    app.get_webview_panel("main").map(|p| p.is_visible()).unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
pub fn show_popup(app: &tauri::AppHandle) {
    use tauri::Manager as _;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg(not(target_os = "macos"))]
pub fn hide_popup(app: &tauri::AppHandle) {
    use tauri::Manager as _;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

#[cfg(not(target_os = "macos"))]
pub fn popup_visible(app: &tauri::AppHandle) -> bool {
    use tauri::Manager as _;
    app.get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Parks the popup along the bottom edge of the screen the pointer is on, full
/// width. The cards read as a shelf sitting on the desktop, and multi-monitor
/// users get it where they are looking, not where the app happens to remember.
pub fn park_at_bottom(window: &tauri::WebviewWindow) {
    let monitor = match window.current_monitor() {
        Ok(Some(m)) => m,
        _ => match window.primary_monitor() {
            Ok(Some(m)) => m,
            _ => return,
        },
    };
    let scale = monitor.scale_factor();
    let screen = monitor.size().to_logical::<f64>(scale);
    let origin = monitor.position().to_logical::<f64>(scale);

    let height = 460.0_f64;
    let width = screen.width;
    let _ = window.set_size(tauri::LogicalSize::new(width, height));
    let _ = window.set_position(tauri::LogicalPosition::new(
        origin.x,
        origin.y + screen.height - height,
    ));
}
