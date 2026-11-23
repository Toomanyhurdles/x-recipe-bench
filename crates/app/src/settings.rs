//! What is remembered between runs.
//!
//! Storage is `craft-settings` from craft-libs: a readable `config.toml` in
//! Linux `~/.config/x-recipe-bench/`, Windows `%APPDATA%\x-recipe-bench\`,
//! saved atomically with a backup, and moved aside (not lost) if damaged.
//! Setting `X_RECIPE_BENCH_CONFIG_DIR` points it at another folder.
//! Versions up to 0.18 kept `settings.json` there; it is read once and left in
//! place, so going back to an older version still works.

use crate::border::Border;
use crate::orientation::Orientation;
use crate::theme::Appearance;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
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
    /// Let the desktop draw the title bar instead of the app (takes effect
    /// at the next start).
    pub system_title_bar: bool,
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
            system_title_bar: false,
        }
    }
}

impl craft_settings::Settings for Settings {
    const APP_ID: &'static str = crate::APP_ID;
    const SCHEMA_VERSION: u32 = 1;
}

/// The settings folder's name while the app was called X Raw Probe.
const OLD_APP_ID: &str = "x-raw-probe";

/// The settings files of versions up to 0.18, newest first.
fn legacy_files() -> Vec<craft_settings::LegacyFile<Settings>> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    let Some(base) = base else { return Vec::new() };
    [crate::APP_ID, OLD_APP_ID]
        .iter()
        .map(|name| craft_settings::LegacyFile { path: base.join(name).join("settings.json"), parse: Settings::from_json })
        .collect()
}

/// The open settings file. Changes are kept in memory by [`Store::save`] and
/// written when they differ from what is on disk.
pub struct Store(craft_settings::SettingsStore<Settings>);

impl Store {
    /// Opens the settings file, or starts from the defaults (or from an older
    /// version's `settings.json`) when there is none. Never fails.
    pub fn open() -> (Store, Vec<String>) {
        Store::open_at(craft_settings::Location::Auto)
    }

    pub fn open_at(location: craft_settings::Location) -> (Store, Vec<String>) {
        let options = craft_settings::OpenOptions::new(location).legacy(legacy_files());
        let (store, report) = craft_settings::SettingsStore::open_with(options);
        let notes = report_notes(&report);
        (Store(store), notes)
    }

    pub fn get(&self) -> &Settings {
        self.0.get()
    }

    /// Remembers `settings` and writes them to disk if they changed.
    pub fn save(&mut self, settings: Settings) {
        if self.0.set(settings) || self.0.is_dirty() {
            let _ = self.0.flush();
        }
    }
}

/// Lines for the Log about anything unusual that happened while loading.
fn report_notes(report: &craft_settings::LoadReport) -> Vec<String> {
    let text = format!("{report:?}");
    // Only mention problems; a normal load says nothing.
    if text.contains("MovedAside") || text.contains("Damaged") || text.contains("ReadOnly") {
        vec![format!("Settings: {text}")]
    } else {
        Vec::new()
    }
}

impl Settings {
    /// Reads a `settings.json` from versions up to 0.18. Entries this version
    /// does not know are ignored; missing ones get their defaults.
    pub fn from_json(bytes: &[u8]) -> Result<Settings, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_survive_saving_and_loading() {
        let dir = std::env::temp_dir().join(format!("xrb-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut settings = Settings::default();
        settings.border = Border { on: true, percent: 7.5, ..Default::default() };
        settings.dark_mode = false;
        settings.photos_place = Some(crate::windows::Place { x: 10.0, y: 20.0, width: 400.0, height: 900.0 });
        settings.orientations.insert("/photos/a b.RAF".into(), Orientation::default().rotated_cw());
        let (mut store, _) = Store::open_at(craft_settings::Location::Dir(dir.clone()));
        store.save(settings.clone());
        assert!(dir.join("config.toml").exists());

        let (store, _) = Store::open_at(craft_settings::Location::Dir(dir.clone()));
        let back = store.get();
        assert_eq!(back.border, settings.border);
        assert!(!back.dark_mode);
        assert_eq!(back.photos_place, settings.photos_place);
        assert_eq!(back.orientations, settings.orientations);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn files_from_older_versions_still_load() {
        // Layouts, tabs and separate windows from before the fixed screen
        // are ignored; everything else is kept.
        let old = r#"{"last_folder":"/photos","layout_version":6,"layouts":[{"dock":null}],"workspace":"Develop",
            "separate_windows":[{"panel":"Editor"}],"saved_layouts":[],"thumbnail_size":200.0,"dark_mode":false}"#;
        let settings = Settings::from_json(old.as_bytes()).unwrap();
        assert_eq!(settings.last_folder, Some(PathBuf::from("/photos")));
        assert!(!settings.dark_mode);
        assert!(Settings::from_json(b"not json").is_err());
    }
}
