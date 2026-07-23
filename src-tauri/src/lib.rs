//! Iago — clipboard history in the menu bar.
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
mod settings;
#[cfg(target_os = "macos")]
mod screenshot;
mod source_app;
mod store;

use history::History;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{MenuBuilder, MenuItem},
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

/// The user's choices, plus the folder they are written to. Built in `setup`,
/// where Tauri hands us the OS-resolved data dir — the path is never spelled out
/// in the code.
struct SettingsState {
    /// Cached: the clipboard watcher consults the retention on every clip and
    /// must not read the disk three times a second.
    current: Arc<Mutex<settings::Settings>>,
    dir: PathBuf,
}

/// What the settings window shows: the chosen retention and the options for it,
/// plus the interface scale and the ends of its slider.
#[tauri::command]
fn get_settings(cfg: tauri::State<SettingsState>) -> serde_json::Value {
    let current = cfg.current.lock().map(|s| *s).unwrap_or_default();
    serde_json::json!({
        "retention_days": current.retention_days,
        "retention_choices": settings::RETENTION_CHOICES,
        "instant_screenshots": instant_state(),
        "ui_scale": current.ui_scale,
        "ui_scale_min": settings::MIN_UI_SCALE,
        "ui_scale_max": settings::MAX_UI_SCALE,
    })
}

/// Change how long clips live. Shortening it takes effect at once — a user who
/// just cut the window from a month to a day expects yesterday's clips gone now,
/// not whenever the next clip happens to arrive.
#[tauri::command]
fn set_retention_days(
    app: AppHandle,
    state: tauri::State<AppState>,
    cfg: tauri::State<SettingsState>,
    store: tauri::State<Arc<store::Store>>,
    days: u32,
) -> Result<(), String> {
    if !settings::RETENTION_CHOICES.contains(&days) {
        return Err(format!("not one of the offered choices: {}", days));
    }
    // Change only the retention: the other settings (the interface scale) live in
    // the same file and must survive a write that is not about them.
    let mut chosen = cfg.current.lock().map(|s| *s).unwrap_or_default();
    chosen.retention_days = days;
    settings::save(&cfg.dir, &chosen)?;
    if let Ok(mut s) = cfg.current.lock() {
        *s = chosen;
    }
    let dropped = match state.history.lock() {
        Ok(mut h) => h.prune_expired(history::now_secs(), chosen.max_age_secs()),
        Err(e) => return Err(e.to_string()),
    };
    if dropped > 0 {
        persist(&store, &state.history);
        let _ = app.emit("history-changed", ());
    }
    debug_log::log(&format!("settings: retention {} days, {} clips dropped", days, dropped));
    Ok(())
}

/// Set how much bigger the interface renders. The frontend applies it as a page
/// zoom; here it is clamped and stored so it holds across launches, and so the
/// next time a sheet opens its window is grown to fit the zoomed content.
#[tauri::command]
fn set_ui_scale(cfg: tauri::State<SettingsState>, scale: f32) -> Result<(), String> {
    let mut chosen = cfg.current.lock().map(|s| *s).unwrap_or_default();
    chosen.ui_scale = settings::clamp_scale(scale);
    settings::save(&cfg.dir, &chosen)?;
    if let Ok(mut s) = cfg.current.lock() {
        *s = chosen;
    }
    debug_log::log(&format!("settings: ui scale {:.2}", chosen.ui_scale));
    Ok(())
}

/// The macOS pref that decides whether a screenshot reaches the clipboard at
/// once or five seconds later, behind the floating thumbnail.
#[tauri::command]
fn set_instant_screenshots(on: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        screenshot::set_instant(on)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = on;
        Ok(())
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

/// ⌦ on a card. The clip leaves the history and, if it was an image, its file
/// leaves the disk — `store::save` sweeps whatever no longer has a clip.
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

/// The zone the user is standing in — the cards or the app icons. The Shortcuts
/// window lights up the one that is live, because the arrows and ⌫ mean different
/// things in each.
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

/// The windows the tray menu opens. They are hidden on close, never destroyed,
/// so the same window answers every time the menu item is pressed.
const TRAY_WINDOWS: [&str; 2] = ["settings", "shortcuts"];

/// The size each sheet is authored at, mirroring tauri.conf.json. The interface
/// scale grows the window from here so the zoomed content is not clipped by a
/// window that stayed at 1×.
fn sheet_base_size(label: &str) -> Option<(f64, f64)> {
    match label {
        "settings" => Some((420.0, 430.0)),
        "shortcuts" => Some((420.0, 510.0)),
        _ => None,
    }
}

fn show_window(app: &AppHandle, label: &str) {
    match app.get_webview_window(label) {
        Some(w) => {
            if let Some((bw, bh)) = sheet_base_size(label) {
                let scale = app
                    .try_state::<SettingsState>()
                    .and_then(|c| c.current.lock().ok().map(|s| s.ui_scale))
                    .unwrap_or(settings::DEFAULT_UI_SCALE) as f64;
                let _ = w.set_size(tauri::LogicalSize::new(bw * scale, bh * scale));
            }
            let _ = w.show();
            let _ = w.set_focus();
        }
        // A destroyed window silently does nothing when its menu item is pressed,
        // which reads to the user as a dead menu. Say so in the log.
        None => debug_log::log(&format!("tray: window '{}' is gone, cannot show it", label)),
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
            get_settings,
            set_retention_days,
            set_ui_scale,
            set_instant_screenshots,
            check_for_update,
            install_update
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // The history outlives the process. An update *is* a restart, and
            // losing the clipboard because a new version arrived is not something
            // a user should have to accept.
            let data_dir = app.path().app_data_dir().map_err(|e| format!("no app data dir: {}", e))?;
            let store = Arc::new(store::Store::new(data_dir.clone()));
            let current = Arc::new(Mutex::new(settings::load(&data_dir)));
            let max_age = current.lock().ok().and_then(|s| s.max_age_secs());
            if let Ok(mut h) = history.lock() {
                // Both of these shrink the history, and what shrank has to reach
                // the disk: a duplicate the index still carries is one the next
                // launch would have to collapse all over again.
                let collapsed = h.restore(store.load());
                // Time passed while the app was not running: whatever expired in
                // the meantime must not come back on screen.
                let dropped = h.prune_expired(history::now_secs(), max_age);
                if collapsed + dropped > 0 {
                    store.save(h.items());
                    debug_log::log(&format!(
                        "history: {} duplicates collapsed, {} expired clips dropped at startup",
                        collapsed, dropped
                    ));
                }
            }
            app.manage(SettingsState { current: Arc::clone(&current), dir: data_dir });
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
            let watcher_settings = Arc::clone(&current);
            let w = clipboard::Watcher::new(Arc::clone(&history), Arc::clone(&skip_next));
            std::thread::spawn(move || {
                w.run(|| {
                    // A new clip is also the moment to sweep the expired ones: the
                    // app sits open for days, so startup alone is not enough.
                    let max_age = watcher_settings.lock().ok().and_then(|s| s.max_age_secs());
                    if let Ok(mut h) = watcher_history.lock() {
                        h.prune_expired(history::now_secs(), max_age);
                    }
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

            // Introduce ourselves to Accessibility at launch rather than at the
            // first paste. The check is what puts the app in the settings pane,
            // and a pane with no Iago in it gives the user nothing to switch on
            // — the paste just dies quietly, which is how the rename broke it.
            if paste::accessibility_trusted(false) {
                debug_log::log("accessibility: granted");
            } else {
                debug_log::log("accessibility: missing — asking for it");
                paste::accessibility_trusted(true);
            }

            debug_log::log("setup complete");
            Ok(())
        })
        .on_window_event(|window, event| {
            // The utility windows close with their cross, but closing must not
            // destroy them — the tray items reopen the same windows, and a
            // destroyed one cannot be shown again. Every window the tray can open
            // belongs here; forgetting one makes its menu item dead after the
            // first close.
            if TRAY_WINDOWS.contains(&window.label()) {
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
        .expect("error while running Iago");
}

/// Menu-bar menu. Mirrors Ribbit/Quill: update first, then the utilities, then
/// the version, then quit.
fn build_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let update = MenuItem::with_id(app, "update", "Check for updates", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let shortcuts = MenuItem::with_id(app, "shortcuts", "Shortcuts", true, None::<&str>)?;
    let version = MenuItem::with_id(
        app,
        "version",
        format!("Iago v{}", env!("CARGO_PKG_VERSION")),
        false,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit Iago", true, None::<&str>)?;

    let menu = MenuBuilder::new(app)
        .item(&update)
        .separator()
        .item(&settings_item)
        .item(&shortcuts)
        .separator()
        .item(&version)
        .item(&quit)
        .build()?;

    // announce_update() rewrites this item's text when a release lands.
    app.manage(update.clone());

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Iago — clipboard history (⌥V)")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "update" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    on_update_clicked(app).await;
                });
            }
            "settings" => show_window(app, "settings"),
            "shortcuts" => show_window(app, "shortcuts"),
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

#[cfg(test)]
mod window_tests {
    use super::TRAY_WINDOWS;

    /// The popup itself. Everything else in the config is a window the tray opens,
    /// and every one of those must survive being closed.
    const POPUP: &str = "main";

    #[test]
    fn every_window_the_tray_opens_is_hidden_on_close_not_destroyed() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let labels = conf["app"]["windows"]
            .as_array()
            .expect("config has no windows")
            .iter()
            .map(|w| w["label"].as_str().expect("window without a label").to_string());

        for label in labels {
            if label == POPUP {
                continue;
            }
            assert!(
                TRAY_WINDOWS.contains(&label.as_str()),
                "window '{}' is not in TRAY_WINDOWS: closing it would destroy it, \
                 and its tray item would then open nothing",
                label
            );
        }
    }
}
