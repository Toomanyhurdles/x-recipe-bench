//! Log: what the app and the camera have been doing.

use crate::state::AppState;
use eframe::egui::{self, Ui};

pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        if ui.button("Copy all").clicked() {
            ui.ctx().copy_text(state.log.join("\n"));
        }
        if ui.button("Clear").clicked() {
            state.log.clear();
        }
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for line in &state.log {
                ui.label(line);
            }
        });
}
