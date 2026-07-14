//! What the user chose, kept between launches.
//!
//! Only one thing lives here so far, and it is the one that matters: how long a
//! clip is allowed to stay. A clipboard history with no expiry is a transcript
//! of everything you ever copied, sitting on disk until fifty newer clips push
//! it out — which for a rarely-copied item can be months. Time, not count, is
//! what a person actually reasons about ("keep a week"), so time is the knob.
//!
//! The file sits next to the history, written owner-only through `private.rs`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE: &str = "settings.json";

/// Days a clip may live. `0` means no expiry — clips only leave when the ring
/// pushes them out. The default is a week: long enough to find last Tuesday's
/// snippet, short enough that the file is not an archive of your year.
pub const DEFAULT_RETENTION_DAYS: u32 = 7;

/// The choices the settings window offers, in order. Anything else is refused:
/// the value drives what gets deleted, so it is a picked option, not free text.
pub const RETENTION_CHOICES: [u32; 4] = [1, 7, 30, 0];

/// How much to grow the whole interface. `1.0` is the size the CSS is authored
/// at; the popup and the two sheets all render at this factor. The default is a
/// touch above 1 because the popup is read at a glance from across the desk, and
/// the authored size is a shade tight for that.
pub const DEFAULT_UI_SCALE: f32 = 1.1;

/// The slider's ends. The ceiling is where the popup's content still fits inside
/// its fixed window without the top row falling off — past it the strip would
/// need to grow too, which is a separate change.
pub const MIN_UI_SCALE: f32 = 0.85;
pub const MAX_UI_SCALE: f32 = 1.25;

/// Keep a scale inside the slider's range. A value from disk or the frontend is
/// not trusted to be sane: it drives a window resize, and an absurd factor would
/// throw the sheet off the screen.
pub fn clamp_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(MIN_UI_SCALE, MAX_UI_SCALE)
    } else {
        DEFAULT_UI_SCALE
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    #[serde(default = "default_retention")]
    pub retention_days: u32,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
}

fn default_retention() -> u32 {
    DEFAULT_RETENTION_DAYS
}

fn default_ui_scale() -> f32 {
    DEFAULT_UI_SCALE
}

impl Default for Settings {
    fn default() -> Self {
        Settings { retention_days: DEFAULT_RETENTION_DAYS, ui_scale: DEFAULT_UI_SCALE }
    }
}

impl Settings {
    /// Seconds a clip may live, or `None` when the user turned expiry off.
    pub fn max_age_secs(&self) -> Option<u64> {
        match self.retention_days {
            0 => None,
            d => Some(u64::from(d) * 24 * 60 * 60),
        }
    }
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// Missing or unreadable settings are not a failure — they are a first launch.
pub fn load(dir: &Path) -> Settings {
    match std::fs::read(path(dir)) {
        Ok(raw) => serde_json::from_slice(&raw).unwrap_or_else(|e| {
            crate::debug_log::log(&format!("settings: unreadable, using defaults: {}", e));
            Settings::default()
        }),
        Err(_) => Settings::default(),
    }
}

pub fn save(dir: &Path, s: &Settings) -> Result<(), String> {
    crate::private::create_dir(dir).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    crate::private::write(&path(dir), &bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cp-settings-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&d);
        crate::private::create_dir(&d).unwrap();
        d
    }

    #[test]
    fn a_first_launch_keeps_a_week() {
        let d = temp_dir("fresh");
        assert_eq!(load(&d).retention_days, 7);
        assert_eq!(load(&d).max_age_secs(), Some(7 * 86_400));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_choice_survives_a_restart() {
        let d = temp_dir("roundtrip");
        save(&d, &Settings { retention_days: 30, ..Default::default() }).unwrap();
        assert_eq!(load(&d).retention_days, 30);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn zero_means_no_expiry() {
        assert_eq!(Settings { retention_days: 0, ..Default::default() }.max_age_secs(), None);
    }

    #[test]
    fn the_ui_scale_survives_a_restart_alongside_retention() {
        let d = temp_dir("scale");
        save(&d, &Settings { retention_days: 1, ui_scale: 1.2 }).unwrap();
        let back = load(&d);
        assert_eq!(back.retention_days, 1);
        assert_eq!(back.ui_scale, 1.2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_old_file_without_a_scale_reads_as_the_default() {
        let d = temp_dir("legacy");
        crate::private::write(&path(&d), br#"{"retention_days":30}"#).unwrap();
        assert_eq!(load(&d).ui_scale, DEFAULT_UI_SCALE);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_scale_off_the_ends_is_pulled_back_onto_the_slider() {
        assert_eq!(clamp_scale(5.0), MAX_UI_SCALE);
        assert_eq!(clamp_scale(0.1), MIN_UI_SCALE);
        assert_eq!(clamp_scale(f32::NAN), DEFAULT_UI_SCALE);
        assert_eq!(clamp_scale(1.15), 1.15);
    }

    #[test]
    fn a_corrupt_file_falls_back_to_the_default_instead_of_dying() {
        let d = temp_dir("corrupt");
        crate::private::write(&path(&d), b"{not json").unwrap();
        assert_eq!(load(&d), Settings::default());
        let _ = std::fs::remove_dir_all(&d);
    }
}
