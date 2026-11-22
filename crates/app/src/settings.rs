//! What is remembered between runs. Stored as readable JSON:
//! Linux `~/.config/x-recipe-bench/settings.json`, Windows `%APPDATA%\x-recipe-bench\`.

use crate::border::Border;
use crate::orientation::Orientation;
use crate::theme::Appearance;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub last_folder: Option<PathBuf>,
    /// Windows snap together and move as a group.
    /// Recipe sample pictures: on or off, the shared photo, and photos
    /// chosen for particular recipes (by recipe file name).
    /// Stars, crops and turns of photos whose folder could not be written to.
    pub photo_notes: HashMap<String, crate::photo::PhotoNote>,
    pub samples_enabled: bool,
    pub sample_photo: Option<PathBuf>,
    pub sample_photos: HashMap<String, PathBuf>,
    /// Where the Photos panel's own window was, when it is undocked.
    pub photos_place: Option<crate::windows::Place>,
    pub snap_windows: bool,
    /// How close, in pixels, edges must be to snap.
    pub snap_distance: f32,
    /// The white border around the photo.
    pub border: Border,
    pub export: crate::export::ExportOptions,
    /// How each photo has been turned, by file path. Only turned photos are listed.
    pub orientations: HashMap<String, Orientation>,
    /// How many copies of the photo the Preview panel shows: 1, 2 or 4.
    pub preview_copies: usize,
    pub thumbnail_size: f32,
    pub develop_on_select: bool,
    pub connect_on_start: bool,
    pub auto_render: bool,
    pub appearance: Appearance,
    pub dark_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            last_folder: None,
            photo_notes: HashMap::new(),
            samples_enabled: false,
            sample_photo: None,
            sample_photos: HashMap::new(),
            photos_place: None,
            snap_windows: true,
            snap_distance: 24.0,
            border: Border::default(),
            export: Default::default(),
            orientations: HashMap::new(),
            preview_copies: 1,
            thumbnail_size: 160.0,
            develop_on_select: true,
            connect_on_start: true,
            auto_render: true,
            appearance: Appearance::UltraModern,
            dark_mode: true,
        }
    }
}

/// The settings folder's name while the app was called X Raw Probe.
const OLD_APP_ID: &str = "x-raw-probe";

fn config_dir() -> Option<PathBuf> {
    config_dir_named(crate::APP_ID)
}

fn config_dir_named(name: &str) -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join(name))
}

impl Settings {
    /// Reads settings from JSON text. Entries this version does not know
    /// (from older versions) are ignored; missing ones get their defaults.
    pub fn from_text(text: &str) -> Settings {
        serde_json::from_str(text).unwrap_or_default()
    }

    /// The settings as JSON text.
    pub fn to_text(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn load() -> Self {
        // Settings saved under the app's old name are carried over once.
        let text = config_dir()
            .and_then(|d| std::fs::read_to_string(d.join("settings.json")).ok())
            .or_else(|| {
                config_dir_named(OLD_APP_ID).and_then(|d| std::fs::read_to_string(d.join("settings.json")).ok())
            });
        match text {
            Some(text) => Settings::from_text(&text),
            None => Settings::default(),
        }
    }

    pub fn save(&self) {
        let Some(dir) = config_dir() else { return };
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        // Write to a temporary file first so a crash cannot leave half a file.
        let tmp = dir.join("settings.json.tmp");
        if std::fs::write(&tmp, self.to_text()).is_ok() {
            let _ = std::fs::rename(tmp, dir.join("settings.json"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_survive_saving_and_loading() {
        let mut settings = Settings::default();
        settings.border = Border { on: true, percent: 7.5, ..Default::default() };
        settings.dark_mode = false;
        settings.photos_place = Some(crate::windows::Place { x: 10.0, y: 20.0, width: 400.0, height: 900.0 });
        let back = Settings::from_text(&settings.to_text());
        assert_eq!(back.border, settings.border);
        assert!(!back.dark_mode);
        assert_eq!(back.photos_place, settings.photos_place);
    }

    #[test]
    fn files_from_older_versions_still_load() {
        // Layouts, tabs and separate windows from before the fixed screen
        // are ignored; everything else is kept.
        let old = r#"{"last_folder":"/photos","layout_version":6,"layouts":[{"dock":null}],"workspace":"Develop",
            "separate_windows":[{"panel":"Editor"}],"saved_layouts":[],"thumbnail_size":200.0,"dark_mode":false}"#;
        let settings = Settings::from_text(old);
        assert_eq!(settings.last_folder, Some(PathBuf::from("/photos")));
        assert!(!settings.dark_mode);
        assert!(Settings::from_text("not json").dark_mode);
    }
}
