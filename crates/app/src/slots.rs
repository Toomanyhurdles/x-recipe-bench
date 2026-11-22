//! What the app knows about the camera's custom slots (C1, C2, …).
//!
//! The camera thread reads and writes the slots (`camera_worker.rs`); the
//! Camera Slots panel shows them (`panels/slots.rs`). The protocol is in
//! the camera crate's `slots.rs`, and the translation between recipes and
//! slot values in the recipes crate's `slot.rs`.

use crate::camera_worker::RecipeJob;
use xrp_camera::slots::SlotRead;
use xrp_recipes::Settings;

/// The category that backups of overwritten slots are filed under.
pub const BACKUP_CATEGORY: &str = "Camera backups";

/// One slot as read from the camera.
#[derive(Clone)]
pub struct SlotInfo {
    pub read: SlotRead,
    /// The slot's look, as recipe settings.
    pub settings: Settings,
}

impl SlotInfo {
    /// "C3".
    pub fn label(&self) -> String {
        format!("C{}", self.read.number)
    }
}

/// An upload the user has asked for and not yet confirmed.
pub struct Upload {
    pub number: u16,
    /// The name to give the slot; the user can change it before confirming.
    pub name: String,
    pub job: RecipeJob,
}

/// What an upload did, for showing to the user.
#[derive(Clone)]
pub struct SlotReport {
    /// True if every setting and the name ended up as asked.
    pub complete: bool,
    pub lines: Vec<String>,
    /// File name of the backup of what the slot held before.
    pub backup: Option<String>,
}

#[derive(Default)]
pub struct SlotsState {
    /// The slots as last read. `None` until they have been read.
    pub list: Option<Vec<SlotInfo>>,
    /// The camera thread is reading or writing slots.
    pub busy: bool,
    /// Why the slots could not be read.
    pub error: Option<String>,
    pub pending: Option<Upload>,
    /// Result of the last upload.
    pub report: Option<SlotReport>,
    /// Result of the last "save to library".
    pub message: Option<String>,
}
