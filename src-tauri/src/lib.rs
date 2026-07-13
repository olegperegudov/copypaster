//! CopyPaster — clipboard history in the menu bar.
//!
//! Shape of the thing: a clipboard watcher and a screenshot watcher feed a ring
//! buffer of clips; Option+V raises a non-activating panel showing them as
//! cards; picking one puts it back on the clipboard and posts Cmd+V into the app
//! the user came from. The panel never takes focus away from that app — see
//! mac_window.

mod clipboard;
mod debug_log;
mod history;
mod mac_window;
mod paste;
mod private;
#[cfg(target_os = "macos")]
mod screenshot;
mod source_app;
mod store;

use history::History;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{CheckMenuItem, MenuBuilder, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_updater::UpdaterExt;

/// Menu-bar icon tinted green while an update is waiting — the same signal
/// Ribbit and Quill give, so the three apps behave alike.
const TRAY_UPDATE_ICON: &[u8] = include_bytes!("../icons/tray-update.png");

const HOTKEY: &str = "alt+v";

struct AppState {
    history: Arc<Mutex<History>>,
    /// Raised right before we write to the clipboard ourselves, so our own write
    /// does not come back through the watcher as a new clip.
    skip_next: Arc<Mutex<bool>>,
    /// The app that was frontmost when the popup opened — where the paste goes.
    target_pid: Mutex<Option<i32>>,
}

#[tauri::command]
fn get_history(state: tauri::State<AppState>) -> Vec<history::ClipView> {
    match state.history.lock() {
        Ok(h) => h.view(),
        Err(_) => Vec::new(),
    }
}

/// Pick a card: clipboard, then paste into the app we came from, then dismiss.
#[tauri::command]
fn pick(app: AppHandle, state: tauri::State<AppState>, id: u64) -> Result<(), String> {
    let payload = {
        let h = state.history.lock().map_err(|e| e.to_string())?;
        h.get(id).ok_or_else(|| format!("clip {} is gone", id))?.payload.clone()
    };
    let target = state.target_pid.lock().ok().and_then(|g| *g);

    // Down first: the panel is over the target window, and the paste must land
    // in the app underneath.
    mac_window::hide_popup(&app);
    paste::paste(&payload, &state.skip_next, target)
}

/// Backspace on a card. The clip leaves the history and, if it was an image, its
/// file leaves the disk — `store::save` sweeps whatever no longer has a clip.
#[tauri::command]
fn delete_clip(
    app: AppHandle,
    state: tauri::State<AppState>,
    store: tauri::State<Arc<store::Store>>,
    id: u64,
) -> Result<(), String> {
    {
        let mut h = state.history.lock().map_err(|e| e.to_string())?;
        if !h.remove(id) {
            return Err(format!("clip {} is gone", id));
        }
    }
    persist(&store, &state.history);
    app.emit("history-changed", ()).map_err(|e| e.to_string())
}

/// Esc, or a click past the popup: put it away and hand the keyboard back to the
/// app the user came from, exactly as a paste would have done.
#[tauri::command]
fn close_popup(app: AppHandle, state: tauri::State<AppState>) {
    mac_window::hide_popup(&app);
    if let Some(pid) = state.target_pid.lock().ok().and_then(|g| *g) {
        paste::activate(pid);
    }
}

#[tauri::command]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn js_log(message: String) {
    debug_log::log(&format!("[ui] {}", message));
}

/// The zone the user is standing in — the Shortcuts window shows the keys that
/// are live right there, because the same key does different things per zone
/// (digits pick a card, or type a digit into the search field).
#[tauri::command]
fn set_zone(app: AppHandle, zone: String) {
    let _ = app.emit("zone-changed", zone);
}

#[tauri::command]
fn get_instant_screenshots() -> bool {
    #[cfg(target_os = "macos")]
    {
        screenshot::instant_enabled()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<serde_json::Value, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            debug_log::log(&format!("update: v{} available", version));
            announce_update(&app, &version);
            Ok(serde_json::json!({ "available": true, "version": version }))
        }
        Ok(None) => {
            debug_log::log("update: up to date");
            Ok(serde_json::json!({ "available": false }))
        }
        Err(e) => {
            debug_log::log(&format!("update: check failed: {}", e));
            Err(e.to_string())
        }
    }
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            debug_log::log(&format!("update: downloading v{}", update.version));
            update
                .download_and_install(|_, _| {}, || debug_log::log("update: downloaded, restarting"))
                .await
                .map_err(|e| e.to_string())?;
            app.restart();
        }
        Ok(None) => Err("No updates available".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// Light the menu-bar icon green and turn the menu's first item into the
/// install action. Called from both the manual check and the background poll.
fn announce_update(app: &AppHandle, version: &str) {
    if let Some(item) = app.try_state::<MenuItem<tauri::Wry>>() {
        let _ = item.set_text(format!("Update to v{}", version));
    }
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(icon) = tauri::image::Image::from_bytes(TRAY_UPDATE_ICON) {
            let _ = tray.set_icon(Some(icon));
        }
    }
    let _ = app.emit("update-available", version);
}

/// Writes the history out after a clip was added. Called on the watcher thread,
/// off the UI path — the popup does not wait for the disk.
fn persist(store: &store::Store, history: &Mutex<History>) {
    match history.lock() {
        Ok(h) => store.save(h.items()),
        Err(e) => debug_log::log(&format!("store: history poisoned, not saved: {}", e)),
    }
}

/// Option+V: raise the popup, or put it away if it is already up.
fn toggle_popup(app: &AppHandle) {
    if mac_window::popup_visible(app) {
        debug_log::log("hotkey: popup down");
        mac_window::hide_popup(app);
        // Same as Esc: whoever we took the keyboard from gets it back.
        if let Some(state) = app.try_state::<AppState>() {
            if let Some(pid) = state.target_pid.lock().ok().and_then(|g| *g) {
                paste::activate(pid);
            }
        }
        return;
    }
    // Remember where to paste *before* the panel goes up.
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut pid) = state.target_pid.lock() {
            *pid = paste::frontmost_pid();
            debug_log::log(&format!("hotkey: popup up, target pid = {:?}", *pid));
        }
    }
    if let Some(window) = app.get_webview_window("main") {
        mac_window::park_at_bottom(&window);
    }
    let _ = app.emit("popup-opened", ());
    mac_window::show_popup(app);
}

fn show_shortcuts(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("shortcuts") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn run() {
    debug_log::init();

    let history = Arc::new(Mutex::new(History::new()));
    let skip_next = Arc::new(Mutex::new(false));

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_nspanel_init())
        .manage(AppState {
            history: Arc::clone(&history),
            skip_next: Arc::clone(&skip_next),
            target_pid: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            get_history,
            pick,
            delete_clip,
            close_popup,
            get_version,
            js_log,
            set_zone,
            get_instant_screenshots,
            check_for_update,
            install_update
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // The history outlives the process. An update *is* a restart, and
            // losing the clipboard because a new version arrived is not something
            // a user should have to accept.
            let store = Arc::new(store::Store::new(
                app.path().app_data_dir().map_err(|e| format!("no app data dir: {}", e))?,
            ));
            if let Ok(mut h) = history.lock() {
                h.restore(store.load());
            }
            // delete_clip writes the index out on the spot, so the store has to be
            // reachable from a command, not just from the watcher threads.
            app.manage(Arc::clone(&store));

            // Menu-bar utility: no Dock icon, no Cmd-Tab entry. Also keeps app
            // activation out of the paste path.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            build_tray(app)?;

            if let Some(window) = app.get_webview_window("main") {
                if let Err(e) = mac_window::setup_panel(&window) {
                    debug_log::log(&format!("panel setup failed: {}", e));
                }
            }
            mac_window::dismiss_on_outside_click(handle.clone());

            // Option+V from anywhere.
            let hotkey: Shortcut = HOTKEY.parse().map_err(|e| format!("bad hotkey: {}", e))?;
            let hk_handle = handle.clone();
            app.global_shortcut().on_shortcut(hotkey, move |_app, _sc, event| {
                // Fire on press only — on_shortcut also reports the release, and
                // acting on both toggles the popup up and straight back down.
                if event.state == ShortcutState::Pressed {
                    toggle_popup(&hk_handle);
                }
            })?;
            debug_log::log(&format!("hotkey registered: {}", HOTKEY));

            // Clipboard watcher.
            let watcher_handle = handle.clone();
            let watcher_history = Arc::clone(&history);
            let watcher_store = Arc::clone(&store);
            let w = clipboard::Watcher::new(Arc::clone(&history), Arc::clone(&skip_next));
            std::thread::spawn(move || {
                w.run(|| {
                    persist(&watcher_store, &watcher_history);
                    let _ = watcher_handle.emit("history-changed", ());
                });
            });

            // Screenshot watcher: a fresh capture lands in the history the
            // instant its file appears, without a trip through the clipboard poll.
            #[cfg(target_os = "macos")]
            {
                let shot_handle = handle.clone();
                let shot_history = Arc::clone(&history);
                let shot_skip = Arc::clone(&skip_next);
                let shot_store = Arc::clone(&store);
                let saved_history = Arc::clone(&history);
                std::thread::spawn(move || {
                    screenshot::watch(shot_history, shot_skip, || {
                        persist(&shot_store, &saved_history);
                        let _ = shot_handle.emit("history-changed", ());
                    });
                });
            }

            // The app sits in the tray all day, so a release that ships while it
            // runs has to light the icon on its own.
            let update_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                loop {
                    if let Ok(updater) = update_handle.updater() {
                        match updater.check().await {
                            Ok(Some(update)) => {
                                debug_log::log(&format!("update: v{} available", update.version));
                                announce_update(&update_handle, &update.version);
                                break; // icon is lit — nothing left to poll for
                            }
                            Ok(None) => {}
                            Err(e) => debug_log::log(&format!("update: poll failed: {}", e)),
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(30 * 60)).await;
                }
            });

            debug_log::log("setup complete");
            Ok(())
        })
        .on_window_event(|window, event| {
            // The cheat sheet closes with its cross, but closing must not destroy
            // it — the tray item reopens the same window, and a destroyed one
            // cannot be shown again.
            if window.label() == "shortcuts" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            // Windows counterpart of the outside-click monitor: there the popup is
            // an ordinary window, so a click on another one takes its focus away.
            #[cfg(not(target_os = "macos"))]
            if window.label() == "main" {
                if let tauri::WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running CopyPaster");
}

/// Menu-bar menu. Mirrors Ribbit/Quill: update first, then the utilities, then
/// the version, then quit.
fn build_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let update = MenuItem::with_id(app, "update", "Check for updates", true, None::<&str>)?;
    let shortcuts = MenuItem::with_id(app, "shortcuts", "Shortcuts", true, None::<&str>)?;
    // Named after what the user gets, not after the macOS pref it flips: ticking
    // it turns off the floating thumbnail, and the capture reaches the clipboard
    // at once instead of five seconds later.
    let instant = CheckMenuItem::with_id(
        app,
        "instant",
        "Screenshot straight to clipboard (no thumbnail)",
        true,
        instant_state(),
        None::<&str>,
    )?;
    let version = MenuItem::with_id(
        app,
        "version",
        format!("CopyPaster v{}", env!("CARGO_PKG_VERSION")),
        false,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit CopyPaster", true, None::<&str>)?;

    let menu = MenuBuilder::new(app)
        .item(&update)
        .separator()
        .item(&shortcuts)
        .item(&instant)
        .separator()
        .item(&version)
        .item(&quit)
        .build()?;

    // announce_update() rewrites this item's text when a release lands.
    app.manage(update.clone());

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("CopyPaster — clipboard history (⌥V)")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "update" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    on_update_clicked(app).await;
                });
            }
            "shortcuts" => show_shortcuts(app),
            "instant" => toggle_instant(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// One menu item, two jobs: check when nothing is pending, install once a
/// version has been found. Two items would leave a dead "Check" sitting next to
/// a live "Update".
async fn on_update_clicked(app: AppHandle) {
    match check_for_update(app.clone()).await {
        Ok(v) if v["available"] == serde_json::Value::Bool(true) => {
            if let Err(e) = install_update(app).await {
                debug_log::log(&format!("update: install failed: {}", e));
            }
        }
        Ok(_) => debug_log::log("update: nothing to install"),
        Err(e) => debug_log::log(&format!("update: check failed: {}", e)),
    }
}

fn instant_state() -> bool {
    #[cfg(target_os = "macos")]
    {
        screenshot::instant_enabled()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn toggle_instant(_app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let now = screenshot::instant_enabled();
        if let Err(e) = screenshot::set_instant(!now) {
            debug_log::log(&format!("screenshot: toggle failed: {}", e));
        }
    }
}

#[cfg(target_os = "macos")]
fn tauri_nspanel_init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_nspanel::init()
}

/// The panel plugin is macOS-only; on Windows the popup is an ordinary
/// always-on-top tool window, so this is a no-op plugin to keep one builder
/// chain instead of two.
#[cfg(not(target_os = "macos"))]
fn tauri_nspanel_init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("noop").build()
}
