//! The data every panel reads and changes.

use crate::camera_worker::{CameraCommand, RecipeJob};
use crate::events::{EmbeddedKind, EventSender, PreviewKey};
use crate::image_worker::ImageRequest;
use eframe::egui::TextureHandle;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use xrp_camera::profile::Profile;
use crate::orientation::Orientation;
use crate::photo::{Crop, Facts, Filter, FolderNotes, PhotoNote};
use std::collections::BTreeSet;
use xrp_camera::exif::Exif;
use xrp_recipes::{Library, Settings, ALL_RECIPES};

#[derive(Debug, Clone, PartialEq)]
pub enum CameraStatus {
    Disconnected,
    Connecting,
    NotFound,
    Connected { model: String, firmware: String, mode: String, ready: bool },
    Busy { holder: Option<String> },
    Failed(String),
}

impl CameraStatus {
    /// Connected and able to develop.
    pub fn ready(&self) -> bool {
        matches!(self, CameraStatus::Connected { ready: true, .. })
    }
    pub fn short_label(&self) -> String {
        match self {
            CameraStatus::Disconnected => "Not connected".into(),
            CameraStatus::Connecting => "Connecting…".into(),
            CameraStatus::NotFound => "No camera found".into(),
            CameraStatus::Connected { model, ready: true, .. } => format!("{model} ready"),
            CameraStatus::Connected { model, .. } => format!("{model}: wrong USB mode"),
            CameraStatus::Busy { .. } => "Camera in use by another program".into(),
            CameraStatus::Failed(_) => "Camera problem".into(),
        }
    }
}

/// A picture that is requested once and then kept.
pub enum Slot {
    Loading,
    Ready(TextureHandle),
    Missing,
}

/// How many camera-developed previews to keep in memory.
const DEVELOPED_CACHE: usize = 24;

/// The most copies of the photo the Preview panel can show side by side.
pub const MAX_COPIES: usize = 4;

/// The look on one copy of the photo in the Preview panel.
///
/// The copy that is being worked on (the "active" one) has its look held
/// live in `recipes.active` and `editor`, where the recipe picker and the
/// Editor change it. The other copies' looks are parked in these.
#[derive(Clone, Default)]
pub struct CopyLook {
    /// Recipe file name, or `None` for no recipe.
    pub recipe: Option<String>,
    pub settings: Settings,
    pub saved: Settings,
    pub name: String,
}

/// Which looks a batch export uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BatchLooks {
    /// The look on the highlighted copy in the Preview.
    #[default]
    Highlighted,
    /// Every look shown in the Preview's copies.
    AllCopies,
}

#[derive(Default)]
pub struct BatchState {
    /// The batch export window is open.
    pub open: bool,
    /// Every photo in the folder, not only the selected one.
    pub all_photos: bool,
    pub looks: BatchLooks,
    /// How many exports the running batch started with.
    pub total: usize,
    pub running: bool,
}

/// Label used for a recipe that has not been saved to a file yet.
pub const NEW_RECIPE: &str = "(new recipe)";

/// What an unaltered copy of the photo is called.
pub const ORIGINAL: &str = "Original";

/// The recipe being adjusted in the Editor panel.
#[derive(Default)]
pub struct EditorState {
    /// The settings as currently shown on the sliders.
    pub settings: Settings,
    /// The settings as last loaded from or saved to the recipe file.
    pub saved: Settings,
    /// Name used by "Save as new".
    pub name: String,
    pub auto_render: bool,
    /// When a slider last moved, while a preview for it is still owed.
    pub pending_since: Option<std::time::Instant>,
    pub message: Option<String>,
    /// Earlier states of the settings, for Undo; and undone ones, for Redo.
    pub undo: Vec<Settings>,
    pub redo: Vec<Settings>,
    /// The settings as the Editor last left them. If they differ when it
    /// next looks, something else changed them (another recipe was picked)
    /// and the history no longer applies.
    pub history_base: Settings,
    /// When the settings were last changed in the Editor; changes close
    /// together (dragging a slider) count as one step.
    pub last_edit: Option<std::time::Instant>,
}

impl EditorState {
    /// True when the sliders differ from the recipe file.
    pub fn dirty(&self) -> bool {
        self.settings != self.saved
    }
}

/// Everything the Recipes panel needs.
pub struct RecipeState {
    /// The recipes folder, or why it could not be opened.
    pub library: Result<Library, String>,
    /// Recipe applied to the selected photo (file name), or `None` for as shot.
    pub active: Option<String>,
    /// Category whose recipes are listed.
    pub category: String,
    pub search: String,
    /// Text being typed for a new category.
    pub new_category: String,
    /// Recipes ticked in the Recipe Manager (file names).
    pub selected: BTreeSet<String>,
    /// Recipe text pasted into the Recipe Manager, and where it should go.
    /// The "Recipe from text" window is open.
    pub paste_open: bool,
    pub paste_text: String,
    pub paste_category: String,
    /// Description being edited: (recipe file name, text).
    pub description_draft: Option<(String, String)>,
    /// Result of the last action, shown at the bottom of the panel.
    pub message: Option<String>,
    /// Notes from the camera about each recipe (file name → notes).
    pub notes: HashMap<String, Vec<String>>,
}

/// What a photo file says about itself, read without the camera.
pub struct PhotoInfo {
    pub file_size: u64,
    /// `None` if the file carries no readable details.
    pub exif: Option<Exif>,
    /// The look it was shot with, when the camera recorded one.
    pub look: Option<Settings>,
}

impl RecipeState {
    pub fn new(library: Result<Library, String>) -> Self {
        RecipeState {
            library,
            active: None,
            category: ALL_RECIPES.to_string(),
            search: String::new(),
            new_category: String::new(),
            selected: BTreeSet::new(),
            paste_open: false,
            paste_text: String::new(),
            paste_category: String::new(),
            description_draft: None,
            message: None,
            notes: HashMap::new(),
        }
    }
}

pub struct AppState {
    pub folder: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    pub selected: Option<PathBuf>,
    /// Details of the selected photo.
    pub info: Option<PhotoInfo>,
    /// How photos have been turned by the user, by file path.
    pub orientations: HashMap<PathBuf, Orientation>,

    pub thumbnails: HashMap<PathBuf, Slot>,
    /// Large embedded picture of the selected file.
    pub embedded: Option<(PathBuf, Slot)>,
    /// Camera-developed previews, newest last.
    pub developed: Vec<(PreviewKey, TextureHandle)>,
    /// Previews the camera has been asked for and not yet delivered.
    pub developing: Vec<PreviewKey>,
    /// Previews that failed, and why.
    pub develop_errors: Vec<(PreviewKey, String)>,
    /// How many copies of the photo the Preview panel shows: 1, 2 or 4.
    pub copies: usize,
    /// Which copy recipes and the Editor apply to.
    pub active_copy: usize,
    /// The looks of the copies that are not the active one (see [`CopyLook`]).
    /// The entry at `active_copy` is unused while that copy is active.
    pub parked: [CopyLook; MAX_COPIES],
    /// The settings the photo last loaded into the camera was shot with.
    pub as_shot: Option<(PathBuf, Profile)>,
    pub recipes: RecipeState,
    pub editor: EditorState,
    /// Number of exports in progress.
    pub exporting: usize,

    pub camera: CameraStatus,
    pub log: Vec<String>,

    pub thumbnail_size: f32,
    pub develop_on_select: bool,
    pub connect_on_start: bool,
    /// Windows snap together and move as a group; and how close edges must be.
    pub snap_windows: bool,
    pub snap_distance: f32,
    /// Stars (1–5) and crops of photos in the open folder.
    pub stars: HashMap<PathBuf, u8>,
    pub crops: HashMap<PathBuf, Crop>,
    /// Shooting details of the photos in the open folder, as they are read.
    pub facts: HashMap<PathBuf, Facts>,
    /// Which photos Browse shows, and in what order.
    pub filter: Filter,
    /// Stars, a crop or a turn changed and the folder's file must be written.
    pub notes_dirty: bool,
    /// False once writing the folder's file has failed; the notes are then
    /// kept in the app's settings (`kept_notes`).
    pub folder_writable: bool,
    /// Notes for photos in folders that cannot be written to, by full path.
    pub kept_notes: HashMap<String, PhotoNote>,
    /// The crop being adjusted in the Preview, if any.
    pub crop_edit: Option<crate::panels::crop::CropEdit>,
    /// Recipe sample pictures.
    pub samples: crate::samples::SamplesState,
    /// The camera's custom slots (Camera Slots panel).
    pub slots: crate::slots::SlotsState,
    /// Resize, quality and file naming for exports.
    pub export: crate::export::ExportOptions,
    /// The batch export window and its progress.
    pub batch: BatchState,
    /// How far the Preview is zoomed in (1 = whole photo) and where it is
    /// looking: the offset of the photo's centre, as a fraction of its size.
    pub zoom: f32,
    pub pan: eframe::egui::Vec2,
    /// Screen points per pixel of the previewed photo at zoom 1 (for "1:1").
    pub fit_scale: Option<f32>,
    /// The white border shown around the photo and added to exports.
    pub border: crate::border::Border,
    pub appearance: crate::theme::Appearance,
    pub dark_mode: bool,
    /// The Settings window is open.
    pub settings_open: bool,

    pub camera_tx: Sender<CameraCommand>,
    pub image_tx: Sender<ImageRequest>,
    pub events: EventSender,
}

impl AppState {
    pub fn log(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        if self.log.len() > 2000 {
            self.log.drain(..500);
        }
    }

    /// The Recipes folder, if it could be opened.
    pub fn recipes_folder(&self) -> Option<PathBuf> {
        self.recipes.library.as_ref().ok().map(|l| l.folder.clone())
    }

    pub fn send_camera(&self, command: CameraCommand) {
        let _ = self.camera_tx.send(command);
    }

    /// Lists the RAF files in `folder` (not its subfolders).
    pub fn open_folder(&mut self, folder: PathBuf) {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&folder)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.is_file()
                            && p.extension()
                                .map(|x| x.to_string_lossy().eq_ignore_ascii_case("raf"))
                                .unwrap_or(false)
                    })
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        self.log(format!("Opened {} ({} RAF files).", folder.display(), files.len()));
        // The folder's notes: stars, crops, turns.
        self.stars.clear();
        self.crops.clear();
        self.facts.clear();
        self.crop_edit = None;
        self.folder_writable = true;
        let note_all = |path: PathBuf, note: &PhotoNote, state: &mut AppState| {
            if note.stars > 0 {
                state.stars.insert(path.clone(), note.stars.min(5));
            }
            if let Some(crop) = note.crop {
                state.crops.insert(path.clone(), crop);
            }
            if !note.orientation.is_identity() {
                state.orientations.insert(path, note.orientation);
            }
        };
        for (name, note) in &FolderNotes::load(&folder).photos {
            note_all(folder.join(name), note, self);
        }
        // Notes kept in the settings because the folder could not be written to.
        for (path, note) in self.kept_notes.clone() {
            let path = PathBuf::from(path);
            if path.parent() == Some(folder.as_path()) {
                note_all(path, &note, self);
            }
        }
        // Turns remembered by earlier versions move into the folder's file.
        self.notes_dirty = files.iter().any(|f| self.orientations.contains_key(f));

        // Read every photo's details in the background, for filters and sorting.
        let (events, list) = (self.events.clone(), files.clone());
        std::thread::spawn(move || {
            for path in list {
                if let Ok(Some(exif)) = xrp_camera::exif::read_file(&path) {
                    events.send(crate::events::AppEvent::Facts(path, Facts::from_exif(&exif)));
                }
            }
        });

        self.folder = Some(folder);
        self.files = files;
        self.thumbnails.clear();
        self.selected = None;
        self.info = None;
        self.embedded = None;
    }

    pub fn stars_of(&self, raf: &Path) -> u8 {
        self.stars.get(raf).copied().unwrap_or(0)
    }

    pub fn set_stars(&mut self, raf: &Path, stars: u8) {
        if stars == 0 {
            self.stars.remove(raf);
        } else {
            self.stars.insert(raf.to_path_buf(), stars.min(5));
        }
        self.notes_dirty = true;
    }

    pub fn crop_of(&self, raf: &Path) -> Option<Crop> {
        self.crops.get(raf).copied()
    }

    /// Sets or (with `None`) removes a photo's crop.
    pub fn set_crop(&mut self, raf: &Path, crop: Option<Crop>) {
        match crop.filter(|c| !c.is_none()) {
            Some(crop) => self.crops.insert(raf.to_path_buf(), crop),
            None => self.crops.remove(raf),
        };
        self.notes_dirty = true;
    }

    /// The photos Browse shows, filtered and in order.
    pub fn visible_files(&self) -> Vec<PathBuf> {
        self.filter.apply(&self.files, |p| self.stars_of(p), &self.facts)
    }

    /// Writes the open folder's notes file. If the folder cannot be written
    /// to, the notes are kept for the app's settings instead.
    pub fn save_notes(&mut self) {
        self.notes_dirty = false;
        let Some(folder) = self.folder.clone() else { return };
        let mut notes = FolderNotes::default();
        for file in &self.files {
            let note = PhotoNote {
                stars: self.stars_of(file),
                crop: self.crop_of(file),
                orientation: self.orientation_of(file),
            };
            if let Some(name) = file.file_name() {
                notes.photos.insert(name.to_string_lossy().to_string(), note);
            }
        }
        let in_folder = |path: &String| Path::new(path).parent() == Some(folder.as_path());
        match notes.save(&folder) {
            Ok(()) => {
                self.folder_writable = true;
                self.kept_notes.retain(|path, _| !in_folder(path));
            }
            Err(problem) => {
                if self.folder_writable {
                    self.log(format!("{problem} Stars, crops and turns for this folder are kept in the app's settings instead."));
                }
                self.folder_writable = false;
                self.kept_notes.retain(|path, _| !in_folder(path));
                for (name, note) in notes.photos {
                    if !note.is_empty() {
                        self.kept_notes.insert(folder.join(name).to_string_lossy().to_string(), note);
                    }
                }
            }
        }
    }

    /// How the user has turned this photo.
    pub fn orientation_of(&self, raf: &Path) -> Orientation {
        self.orientations.get(raf).copied().unwrap_or_default()
    }

    /// Changes how the selected photo is turned.
    pub fn turn_selected(&mut self, change: impl Fn(Orientation) -> Orientation) {
        let Some(raf) = self.selected.clone() else { return };
        let turned = change(self.orientation_of(&raf));
        // A crop is drawn on the photo as it stands; turned, it no longer fits.
        if self.crops.remove(&raf).is_some() {
            self.log("The crop was removed because the photo was turned.");
        }
        self.notes_dirty = true;
        if turned.is_identity() {
            self.orientations.remove(&raf);
        } else {
            self.orientations.insert(raf, turned);
        }
    }

    pub fn select(&mut self, raf: &Path) {
        if self.selected.as_deref() == Some(raf) {
            return;
        }
        self.selected = Some(raf.to_path_buf());
        // The details sit at the start of the file, so reading them is quick.
        let exif = xrp_camera::exif::read_file(raf).ok().flatten();
        self.info = Some(PhotoInfo {
            file_size: std::fs::metadata(raf).map(|m| m.len()).unwrap_or(0),
            look: exif.as_ref().and_then(xrp_recipes::from_photo::look),
            exif,
        });
        self.embedded = Some((raf.to_path_buf(), Slot::Loading));
        let _ = self.image_tx.send(ImageRequest { raf: raf.to_path_buf(), kind: EmbeddedKind::Large });
        if self.develop_on_select {
            self.request_preview();
        }
    }

    /// Moves the selection by `step` files (negative = earlier).
    pub fn select_relative(&mut self, step: isize) {
        let files = self.visible_files();
        if files.is_empty() {
            return;
        }
        let current = self.selected.as_ref().and_then(|s| files.iter().position(|f| f == s));
        let next = match current {
            Some(i) => (i as isize + step).clamp(0, files.len() as isize - 1) as usize,
            None => 0,
        };
        let path = files[next].clone();
        self.select(&path);
    }

    /// The look on copy `index`: live for the active copy, parked otherwise.
    pub fn look_of(&self, index: usize) -> CopyLook {
        if index == self.active_copy {
            CopyLook {
                recipe: self.recipes.active.clone(),
                settings: self.editor.settings.clone(),
                saved: self.editor.saved.clone(),
                name: self.editor.name.clone(),
            }
        } else {
            self.parked[index.min(MAX_COPIES - 1)].clone()
        }
    }

    /// Makes copy `index` the one that recipes and the Editor apply to.
    pub fn set_active_copy(&mut self, index: usize) {
        let index = index.min(self.copies.saturating_sub(1));
        if index == self.active_copy {
            return;
        }
        self.parked[self.active_copy] = self.look_of(self.active_copy);
        let look = std::mem::take(&mut self.parked[index]);
        self.active_copy = index;
        self.recipes.active = look.recipe;
        self.editor.settings = look.settings;
        self.editor.saved = look.saved;
        self.editor.name = look.name;
        self.editor.pending_since = None;
        self.editor.message = None;
        self.recipes.description_draft = None;
    }

    /// Shows 1, 2 or 4 copies of the photo.
    pub fn set_copies(&mut self, copies: usize) {
        let copies = match copies {
            0 | 1 => 1,
            2 | 3 => 2,
            _ => 4,
        };
        if self.active_copy >= copies {
            // The highlighted copy would disappear. Keep its look instead:
            // it becomes the first copy, replacing the look that was there.
            // Its old place is left with no recipe.
            self.parked[self.active_copy] = CopyLook::default();
            self.active_copy = 0;
        }
        self.copies = copies;
        if self.develop_on_select {
            self.request_preview();
        }
    }

    /// The recipe copy `index` is developed with: its settings as they stand,
    /// which are its recipe's unless they have been changed. `None` = original.
    pub fn job_of(&self, index: usize) -> Option<RecipeJob> {
        let look = self.look_of(index);
        if look.recipe.is_none() && look.settings.is_empty() {
            return None;
        }
        Some(RecipeJob {
            file_name: look.recipe.clone().unwrap_or_else(|| NEW_RECIPE.to_string()),
            name: self.look_name_of(index),
            settings: look.settings,
        })
    }

    /// The recipe the active copy is developed with.
    pub fn active_job(&self) -> Option<RecipeJob> {
        self.job_of(self.active_copy)
    }

    /// Name of the look on copy `index`, for labels and export file names.
    pub fn look_name_of(&self, index: usize) -> String {
        let look = self.look_of(index);
        let base = match &look.recipe {
            Some(file_name) => self
                .recipes
                .library
                .as_ref()
                .ok()
                .and_then(|l| l.find(file_name))
                .map(|r| r.display_name())
                .unwrap_or_else(|| file_name.clone()),
            None if look.settings.is_empty() => return ORIGINAL.to_string(),
            None if !look.name.trim().is_empty() => look.name.trim().to_string(),
            None => "New recipe".to_string(),
        };
        if look.recipe.is_some() && look.settings != look.saved {
            format!("{base} (edited)")
        } else {
            base
        }
    }

    /// Name of the active copy's look.
    pub fn look_name(&self) -> String {
        self.look_name_of(self.active_copy)
    }

    /// The selected photo with the look of copy `index`.
    pub fn key_of(&self, index: usize) -> Option<PreviewKey> {
        let job = self.job_of(index);
        Some(PreviewKey {
            raf: self.selected.clone()?,
            recipe: job.as_ref().map(|j| j.file_name.clone()),
            variant: job.as_ref().map(|j| j.settings.fingerprint()).unwrap_or(0),
        })
    }

    /// The selected photo with the active copy's look.
    pub fn current_key(&self) -> Option<PreviewKey> {
        self.key_of(self.active_copy)
    }

    pub fn developed_for(&self, key: &PreviewKey) -> Option<&TextureHandle> {
        self.developed.iter().find(|(k, _)| k == key).map(|(_, t)| t)
    }

    pub fn store_developed(&mut self, key: PreviewKey, texture: TextureHandle) {
        self.developed.retain(|(k, _)| k != &key);
        self.developed.push((key, texture));
        if self.developed.len() > DEVELOPED_CACHE {
            drop(self.developed.remove(0));
        }
    }

    /// Forgets previews made with recipes, after recipe files changed.
    pub fn forget_recipe_previews(&mut self) {
        self.developed.retain(|(k, _)| k.recipe.is_none());
    }

    /// Asks which recipe files to import. Several can be chosen at once.
    pub fn pick_recipes_to_import(&self) {
        let events = self.events.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Import recipe files")
                .add_filter("X RAW Studio recipes", &["FP1", "fp1"])
                .pick_files();
            if let Some(files) = picked {
                events.send(crate::events::AppEvent::RecipesPicked(files));
            }
        });
    }

    /// Asks for a folder; every recipe file in it, at any depth, is imported.
    pub fn pick_recipe_folder_to_import(&self) {
        let events = self.events.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new().set_title("Import a folder of recipes").pick_folder();
            if let Some(folder) = picked {
                events.send(crate::events::AppEvent::RecipesPicked(vec![folder]));
            }
        });
    }

    /// Asks which photos to read recipes from.
    pub fn pick_photos_for_recipes(&self) {
        let events = self.events.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Make recipes from Fujifilm photos")
                .add_filter("Fujifilm photos", &["JPG", "jpg", "JPEG", "jpeg", "RAF", "raf"])
                .pick_files();
            if let Some(files) = picked {
                events.send(crate::events::AppEvent::PhotosPickedForRecipes(files));
            }
        });
    }

    /// Asks where to copy the ticked recipes to.
    pub fn pick_recipe_export_folder(&self) {
        let events = self.events.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new().set_title("Copy the selected recipes to a folder").pick_folder();
            if let Some(folder) = picked {
                events.send(crate::events::AppEvent::RecipeExportFolderPicked(folder));
            }
        });
    }

    /// Makes a recipe from each photo's recorded look.
    pub fn recipes_from_photos(&mut self, photos: &[PathBuf]) {
        let Ok(library) = &mut self.recipes.library else { return };
        let (mut made, mut skipped) = (Vec::new(), Vec::new());
        for photo in photos {
            let shown = photo.file_name().unwrap_or_default().to_string_lossy().to_string();
            let exif = xrp_camera::exif::read_file(photo).ok().flatten();
            let Some(look) = exif.as_ref().and_then(xrp_recipes::from_photo::look) else {
                skipped.push(shown);
                continue;
            };
            let stem = photo.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let name = format!("{stem} look");
            let device = exif.map(|e| e.model).unwrap_or_default();
            let text =
                xrp_recipes::Recipe::build(&name, &device, &format!("Read from {shown}."), &look.to_fields());
            match library.create(&name, &text) {
                Ok(file_name) => made.push(file_name),
                Err(problem) => skipped.push(format!("{shown} ({problem})")),
            }
        }
        let mut text = match made.len() {
            0 => "No recipes were made.".to_string(),
            1 => format!("Made {}.", made[0]),
            n => format!("Made {n} recipes."),
        };
        if !skipped.is_empty() {
            text.push_str(&format!(" No Fujifilm settings found in: {}.", skipped.join(", ")));
        }
        self.recipes.message = Some(text.clone());
        self.log(text);
    }

    /// Copies the ticked recipes into `dest`.
    pub fn export_selected_recipes(&mut self, dest: &Path) {
        let Ok(library) = &self.recipes.library else { return };
        let names: Vec<String> = self.recipes.selected.iter().cloned().collect();
        let text = match library.export_copies(&names, dest) {
            Ok(n) => format!("Copied {n} recipes to {}.", dest.display()),
            Err(problem) => problem,
        };
        self.recipes.message = Some(text.clone());
        self.log(text);
    }

    /// Copies recipe files into the library and reports the result.
    pub fn import_recipes(&mut self, files: &[PathBuf]) {
        let Ok(library) = &mut self.recipes.library else { return };
        let (added, problems) = library.import(files);
        let mut text = match added.len() {
            0 => "No recipes were imported.".to_string(),
            1 => format!("Imported {}.", added[0]),
            n => format!("Imported {n} recipes."),
        };
        for problem in &problems {
            text.push_str(&format!(" Skipped {problem}."));
        }
        self.recipes.message = Some(text.clone());
        self.log(text);
    }

    /// Makes `recipe` (a file name, or `None` for as shot) the active one and
    /// loads its settings into the editor, discarding unsaved edits.
    pub fn use_recipe(&mut self, recipe: Option<String>) {
        let entry = recipe
            .as_ref()
            .and_then(|f| self.recipes.library.as_ref().ok().and_then(|l| l.find(f)));
        let settings = entry.map(|e| e.settings.clone()).unwrap_or_default();
        self.editor.name = entry.map(|e| e.display_name()).unwrap_or_default();
        self.editor.saved = settings.clone();
        self.editor.settings = settings;
        self.editor.pending_since = None;
        self.editor.message = None;
        self.recipes.active = recipe;
        self.recipes.description_draft = None;
        if self.develop_on_select {
            self.request_preview();
        }
    }

    /// After recipe files changed on disk: reload each copy's recipe, unless
    /// that copy has unsaved edits.
    pub fn refresh_editor_from_library(&mut self) {
        let Ok(library) = &self.recipes.library else { return };
        let refresh = |look: &mut CopyLook| {
            let Some(file_name) = look.recipe.clone() else { return };
            match library.find(&file_name) {
                Some(entry) if look.settings == look.saved => {
                    look.saved = entry.settings.clone();
                    look.settings = entry.settings.clone();
                }
                Some(entry) => look.saved = entry.settings.clone(),
                // The file is gone: keep the settings as an unsaved recipe.
                None => look.recipe = None,
            }
        };
        for (index, parked) in self.parked.iter_mut().enumerate() {
            if index != self.active_copy {
                refresh(parked);
            }
        }
        let mut live = CopyLook {
            recipe: self.recipes.active.clone(),
            settings: self.editor.settings.clone(),
            saved: self.editor.saved.clone(),
            name: self.editor.name.clone(),
        };
        refresh(&mut live);
        self.recipes.active = live.recipe;
        self.editor.settings = live.settings;
        self.editor.saved = live.saved;
    }

    /// Asks the camera for the preview of every copy shown that it has not
    /// made yet, if that makes sense now.
    pub fn request_preview(&mut self) {
        if !self.camera.ready() {
            return;
        }
        for index in 0..self.copies {
            let Some(key) = self.key_of(index) else { return };
            if self.developed_for(&key).is_some() || self.developing.contains(&key) {
                continue;
            }
            self.develop_errors.retain(|(k, _)| k != &key);
            self.developing.push(key.clone());
            let recipe = self.job_of(index);
            self.send_camera(CameraCommand::Preview { slot: index, raf: key.raf, recipe });
        }
    }

    /// What is done to an export of `raf` after the camera has made it.
    pub fn finish_for(&self, raf: &Path) -> crate::export::Finish {
        crate::export::Finish {
            orientation: self.orientation_of(raf),
            crop: self.crop_of(raf),
            border: self.border,
            options: self.export.clone(),
        }
    }

    /// The looks a batch export would use, as the batch window is set.
    /// `None` is the original look.
    pub fn batch_looks(&self) -> Vec<Option<RecipeJob>> {
        let mut looks: Vec<Option<RecipeJob>> = match self.batch.looks {
            BatchLooks::Highlighted => vec![self.active_job()],
            BatchLooks::AllCopies => (0..self.copies).map(|i| self.job_of(i)).collect(),
        };
        // The same look twice would only make the same file twice.
        let mut seen: Vec<(Option<String>, u64)> = Vec::new();
        looks.retain(|look| {
            let id = (look.as_ref().map(|j| j.name.clone()), look.as_ref().map(|j| j.settings.fingerprint()).unwrap_or(0));
            let new = !seen.contains(&id);
            seen.push(id);
            new
        });
        looks
    }

    /// The photos a batch export would cover.
    pub fn batch_photos(&self) -> Vec<PathBuf> {
        if self.batch.all_photos {
            self.visible_files()
        } else {
            self.selected.clone().into_iter().collect()
        }
    }

    pub fn pick_batch_folder(&self) {
        let events = self.events.clone();
        let start = self.folder.clone();
        // For automated tests, which cannot work a folder dialog.
        if let Some(dest) = std::env::var_os("XRP_BATCH_DEST") {
            events.send(crate::events::AppEvent::BatchFolderPicked(PathBuf::from(dest)));
            return;
        }
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new().set_title("Folder for the exported JPEGs");
            if let Some(dir) = start {
                dialog = dialog.set_directory(dir);
            }
            if let Some(dest) = dialog.pick_folder() {
                events.send(crate::events::AppEvent::BatchFolderPicked(dest));
            }
        });
    }

    /// Queues one export per photo and look into `dest`. Files already
    /// there are never replaced: a number is added to the name instead.
    pub fn start_batch(&mut self, dest: &Path) {
        let (photos, looks) = (self.batch_photos(), self.batch_looks());
        let mut taken: Vec<PathBuf> = Vec::new();
        let mut queued = 0;
        for raf in &photos {
            let stem = raf.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "photo".into());
            for look in &looks {
                let name = self.export.file_name(&stem, look.as_ref().map(|j| j.name.as_str()));
                let mut path = dest.join(&name);
                let mut n = 2;
                while path.exists() || taken.contains(&path) {
                    path = dest.join(format!("{} ({n}).jpg", name.trim_end_matches(".jpg")));
                    n += 1;
                }
                taken.push(path.clone());
                self.send_camera(CameraCommand::Export {
                    raf: raf.clone(),
                    recipe: look.clone(),
                    dest: path,
                    finish: self.finish_for(raf),
                });
                queued += 1;
            }
        }
        self.exporting += queued;
        self.batch.total = queued;
        self.batch.running = queued > 0;
        self.log(format!("Batch export: {queued} JPEGs to {}…", dest.display()));
    }

    /// Asks where to save, then exports the selected file at full size.
    pub fn export_selected(&mut self) {
        let Some(raf) = self.selected.clone() else { return };
        let recipe = self.active_job();
        let finish = self.finish_for(&raf);
        let options = self.export.clone();
        let events = self.events.clone();
        // File dialogs block, so they run on their own thread.
        std::thread::spawn(move || {
            let stem = raf.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "export".into());
            let name = options.file_name(&stem, recipe.as_ref().map(|j| j.name.as_str()));
            let mut dialog = rfd::FileDialog::new()
                .set_title("Export full-size JPEG")
                .set_file_name(name)
                .add_filter("JPEG", &["jpg", "jpeg"]);
            if let Some(dir) = raf.parent() {
                dialog = dialog.set_directory(dir);
            }
            if let Some(dest) = dialog.save_file() {
                events.send(crate::events::AppEvent::ExportDestPicked { raf, recipe, dest, finish });
            }
        });
    }

    pub fn pick_folder(&self) {
        let events = self.events.clone();
        let start = self.folder.clone();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new().set_title("Open a folder of RAF files");
            if let Some(dir) = start {
                dialog = dialog.set_directory(dir);
            }
            if let Some(folder) = dialog.pick_folder() {
                events.send(crate::events::AppEvent::FolderPicked(folder));
            }
        });
    }
}
