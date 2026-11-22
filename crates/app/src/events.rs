//! Messages from background threads to the window.

use crate::state::CameraStatus;
use eframe::egui::ColorImage;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

pub enum AppEvent {
    /// The camera's connection state changed.
    Camera(CameraStatus),
    /// The camera finished a preview. `notes` are things the user should know,
    /// such as recipe settings the camera skipped.
    Developed { key: PreviewKey, image: ColorImage, seconds: f32, notes: Vec<String> },
    /// A preview request was dropped because a newer one replaced it.
    PreviewSkipped(PreviewKey),
    /// A preview or export failed.
    DevelopFailed { key: PreviewKey, message: String, export: bool },
    /// The camera loaded `raf`; this is the profile it was shot with.
    AsShot { raf: PathBuf, profile: xrp_camera::profile::Profile },
    /// A full-size image was written to `dest`.
    Exported { dest: PathBuf, seconds: f32 },
    /// A picture embedded in a RAF was decoded. `None` means it could not be.
    Embedded { raf: PathBuf, kind: EmbeddedKind, image: Option<ColorImage> },
    /// The user picked a folder in the folder dialog.
    FolderPicked(PathBuf),
    /// The user picked where to save an export.
    ExportDestPicked {
        raf: PathBuf,
        recipe: Option<crate::camera_worker::RecipeJob>,
        dest: PathBuf,
        finish: crate::export::Finish,
    },
    /// The user picked the folder for a batch export.
    BatchFolderPicked(PathBuf),
    /// Exports that had not started were dropped; this many.
    ExportsCancelled(usize),
    /// The user picked recipe files, or a folder of them, to import.
    RecipesPicked(Vec<PathBuf>),
    /// The user picked photos to read recipes from.
    PhotosPickedForRecipes(Vec<PathBuf>),
    /// The user picked a folder to copy the selected recipes into.
    RecipeExportFolderPicked(PathBuf),
    /// The camera's custom slots were read, or why they could not be.
    Slots(Result<Vec<crate::slots::SlotInfo>, String>),
    /// An upload to a custom slot finished (or failed before changing anything).
    SlotWritten(Result<crate::slots::SlotReport, String>),
    /// The camera finished (or failed) a recipe's sample picture.
    SampleMade { recipe_file: String, result: Result<(), String> },
    /// A photo's shooting details were read, for filtering and sorting.
    Facts(PathBuf, crate::photo::Facts),
    /// A line for the Log panel.
    Log(String),
}

/// Identifies one developed picture: a photo and the recipe applied to it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PreviewKey {
    pub raf: PathBuf,
    /// Recipe file name (or a label for an unsaved one), `None` for as shot.
    pub recipe: Option<String>,
    /// Fingerprint of the settings used, so an edited recipe gets its own
    /// preview. 0 for as shot.
    pub variant: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmbeddedKind {
    /// Small, for the library grid.
    Thumbnail,
    /// Large, for the Preview panel.
    Large,
}

/// Sends events to the window and wakes it up to show them.
#[derive(Clone)]
pub struct EventSender {
    tx: Sender<AppEvent>,
    ctx: eframe::egui::Context,
}

impl EventSender {
    pub fn new(tx: Sender<AppEvent>, ctx: eframe::egui::Context) -> Self {
        EventSender { tx, ctx }
    }
    pub fn send(&self, event: AppEvent) {
        let _ = self.tx.send(event);
        self.ctx.request_repaint();
    }
    pub fn log(&self, line: impl Into<String>) {
        self.send(AppEvent::Log(line.into()));
    }
}
