//! Sample pictures: a small photo for each recipe, showing what it does.
//!
//! Only the camera can apply a recipe, so each sample is made by having
//! the camera develop a reference photo with the recipe. That takes a few
//! seconds per recipe, once; the result is kept as a small JPEG in the
//! `Samples` folder inside the Recipes folder and shown from there, with
//! or without the camera. The feature is off unless switched on in Settings.
//!
//! A sample is made again when its recipe's settings change or a different
//! photo is chosen for it. `index.json` in the folder records, per recipe,
//! which settings and photo its sample was made from.
//!
//! Who does what: the window decides which sample is needed next and asks
//! for one at a time, when nothing else is waiting for the camera
//! (`App::make_samples`); the camera thread develops and saves it.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Longer side of a sample picture, in pixels.
pub const SAMPLE_SIZE: u32 = 520;

/// What one sample was made from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Made {
    /// Fingerprint of the recipe's settings.
    pub settings: u64,
    /// The photo that was developed.
    pub photo: String,
}

/// Which samples exist and what they were made from, by recipe file name.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Index {
    pub samples: HashMap<String, Made>,
}

/// The folder samples are kept in.
pub fn folder(recipes_folder: &Path) -> PathBuf {
    recipes_folder.join("Samples")
}

/// The picture file for a recipe's sample.
pub fn picture(recipes_folder: &Path, recipe_file: &str) -> PathBuf {
    folder(recipes_folder).join(format!("{recipe_file}.jpg"))
}

impl Index {
    pub fn load(recipes_folder: &Path) -> Index {
        std::fs::read_to_string(folder(recipes_folder).join("index.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, recipes_folder: &Path) {
        let dir = folder(recipes_folder);
        if std::fs::create_dir_all(&dir).is_ok() {
            if let Ok(text) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(dir.join("index.json"), text);
            }
        }
    }

    /// True if the recipe has a sample made from exactly these settings and photo.
    pub fn is_current(&self, recipes_folder: &Path, recipe_file: &str, settings: u64, photo: &Path) -> bool {
        self.samples.get(recipe_file).is_some_and(|made| {
            made.settings == settings && Path::new(&made.photo) == photo && picture(recipes_folder, recipe_file).is_file()
        })
    }
}

/// The feature's settings and working state.
#[derive(Default)]
pub struct SamplesState {
    pub enabled: bool,
    /// The photo used for every recipe that has none of its own.
    pub shared_photo: Option<PathBuf>,
    /// Photos chosen for particular recipes, by recipe file name.
    pub own_photos: HashMap<String, PathBuf>,
    pub index: Index,
    /// The sample the camera is making now: recipe file, settings, photo.
    pub in_flight: Option<(String, u64, PathBuf)>,
    /// Samples that failed, so they are not tried over and over.
    pub failed: Vec<(String, u64, PathBuf)>,
    /// Pictures loaded for showing, by recipe file name.
    pub textures: HashMap<String, eframe::egui::TextureHandle>,
    /// Recipe files whose picture could not be loaded.
    pub unreadable: Vec<String>,
    /// The "are you sure" window before switching the feature on.
    pub asking: bool,
}

impl SamplesState {
    /// The photo a recipe's sample is made from.
    pub fn photo_for(&self, recipe_file: &str) -> Option<&PathBuf> {
        self.own_photos.get(recipe_file).or(self.shared_photo.as_ref())
    }

    /// The loaded sample picture of a recipe, if the feature is on.
    pub fn texture(&self, recipe_file: &str) -> Option<&eframe::egui::TextureHandle> {
        if self.enabled {
            self.textures.get(recipe_file)
        } else {
            None
        }
    }

    /// Forgets a recipe's loaded picture, so it is read from disk again.
    pub fn reload(&mut self, recipe_file: &str) {
        self.textures.remove(recipe_file);
        self.unreadable.retain(|f| f != recipe_file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sample_is_current_only_for_the_same_settings_and_photo() {
        let dir = std::env::temp_dir().join(format!("xrb-samples-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(folder(&dir)).unwrap();
        let mut index = Index::default();
        index.samples.insert("Campfire.FP1".into(), Made { settings: 42, photo: "/photos/a.RAF".into() });
        let photo = Path::new("/photos/a.RAF");
        // Recorded, but the picture file is missing.
        assert!(!index.is_current(&dir, "Campfire.FP1", 42, photo));
        std::fs::write(picture(&dir, "Campfire.FP1"), b"jpeg").unwrap();
        assert!(index.is_current(&dir, "Campfire.FP1", 42, photo));
        assert!(!index.is_current(&dir, "Campfire.FP1", 43, photo), "settings changed");
        assert!(!index.is_current(&dir, "Campfire.FP1", 42, Path::new("/photos/b.RAF")), "another photo");
        assert!(!index.is_current(&dir, "Other.FP1", 42, photo));
        // The record survives saving and loading.
        index.save(&dir);
        assert_eq!(Index::load(&dir).samples, index.samples);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_recipe_uses_its_own_photo_before_the_shared_one() {
        let mut s = SamplesState { shared_photo: Some("/p/shared.RAF".into()), ..Default::default() };
        s.own_photos.insert("A.FP1".into(), "/p/own.RAF".into());
        assert_eq!(s.photo_for("A.FP1"), Some(&PathBuf::from("/p/own.RAF")));
        assert_eq!(s.photo_for("B.FP1"), Some(&PathBuf::from("/p/shared.RAF")));
        s.shared_photo = None;
        assert_eq!(s.photo_for("B.FP1"), None);
    }
}
