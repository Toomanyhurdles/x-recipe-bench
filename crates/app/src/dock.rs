//! The panels of the app's one fixed screen.
//!
//! From left to right: Photos; a strip with Info, Camera, Camera Slots and
//! Log; the Preview; the Recipes list; the Editor. Every panel is always
//! shown and none can be closed or rearranged. The Photos panel alone can
//! be undocked into a window of its own (see `App::photos_window`).

use crate::panels;
use crate::state::AppState;
use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    Library,
    Info,
    Camera,
    CameraSlots,
    Log,
    Preview,
    Recipes,
    Editor,
}

impl Panel {
    pub fn title(self) -> &'static str {
        match self {
            Panel::Library => "Photos",
            Panel::Info => "Info",
            Panel::Camera => "Camera",
            Panel::CameraSlots => "Camera Slots",
            Panel::Log => "Log",
            Panel::Preview => "Preview",
            Panel::Recipes => "Recipes",
            Panel::Editor => "Editor",
        }
    }
}

/// Draws a panel's contents.
pub fn show_panel(panel: Panel, ui: &mut egui::Ui, state: &mut AppState) {
    match panel {
        Panel::Library => panels::library::show(ui, state),
        Panel::Info => panels::info::show(ui, state),
        Panel::Camera => panels::camera::show(ui, state),
        Panel::CameraSlots => panels::slots::show(ui, state),
        Panel::Log => panels::log::show(ui, state),
        Panel::Preview => panels::preview::show(ui, state),
        Panel::Recipes => panels::recipes::show(ui, state),
        Panel::Editor => panels::editor::show(ui, state),
    }
}

/// A panel's name as a small heading at the top of its area.
pub fn heading(ui: &mut egui::Ui, panel: Panel) {
    ui.label(egui::RichText::new(panel.title()).strong().color(ui.visuals().weak_text_color()));
}
