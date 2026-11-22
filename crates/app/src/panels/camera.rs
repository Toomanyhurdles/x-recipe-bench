//! Camera: connection state and what to do when it is not ready.

use super::hint;
use crate::camera_worker::CameraCommand;
use crate::state::{AppState, CameraStatus};
use eframe::egui::{self, Ui};
use xrp_camera::busy;

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let status = state.camera.clone();
    ui.horizontal(|ui| {
        let colour = match &status {
            CameraStatus::Connected { ready: true, .. } => egui::Color32::from_rgb(90, 190, 110),
            CameraStatus::Connecting => egui::Color32::from_rgb(220, 180, 70),
            CameraStatus::Disconnected | CameraStatus::NotFound => egui::Color32::from_gray(130),
            _ => egui::Color32::from_rgb(225, 110, 90),
        };
        let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(dot.center(), 5.0, colour);
        ui.strong(status.short_label());
    });
    ui.add_space(6.0);

    match &status {
        CameraStatus::Connected { model, firmware, mode, ready } => {
            egui::Grid::new("camera_details").num_columns(2).show(ui, |ui| {
                ui.label("Model");
                ui.label(model);
                ui.end_row();
                ui.label("Firmware");
                ui.label(firmware);
                ui.end_row();
                ui.label("USB mode");
                ui.label(mode);
                ui.end_row();
            });
            if !ready {
                ui.add_space(6.0);
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "On the camera, set Connection Mode to USB RAW CONV./BACKUP RESTORE, \
                     then disconnect and connect again.",
                );
            }
            ui.add_space(8.0);
            if ui.button("Disconnect").clicked() {
                state.send_camera(CameraCommand::Disconnect);
            }
        }
        CameraStatus::Connecting => {
            ui.spinner();
        }
        CameraStatus::Busy { holder } => {
            match holder {
                Some(h) => ui.label(format!("In use by: {h}")),
                None => ui.label("Another program has the camera open."),
            };
            let desktop = holder.as_deref().map(busy::is_desktop_service).unwrap_or(false);
            if desktop {
                hint(
                    ui,
                    "That is your desktop's automatic camera mounting. Freeing the camera \
                     stops it until you next log in.",
                );
                if ui.button("Free the camera and connect").clicked() {
                    state.send_camera(CameraCommand::FreeFromDesktopService);
                }
            } else {
                hint(ui, "Close that program, then try again.");
            }
            if ui.button("Try again").clicked() {
                state.send_camera(CameraCommand::Connect);
            }
        }
        CameraStatus::NotFound => {
            hint(
                ui,
                "Switch the camera on, set Connection Mode to USB RAW CONV./BACKUP RESTORE, \
                 and plug it in.",
            );
            if ui.button("Connect").clicked() {
                state.send_camera(CameraCommand::Connect);
            }
        }
        CameraStatus::Failed(message) => {
            ui.colored_label(ui.visuals().warn_fg_color, message);
            if ui.button("Connect").clicked() {
                state.send_camera(CameraCommand::Connect);
            }
        }
        CameraStatus::Disconnected => {
            if ui.button("Connect").clicked() {
                state.send_camera(CameraCommand::Connect);
            }
        }
    }

    ui.add_space(10.0);
    ui.separator();
    ui.checkbox(&mut state.connect_on_start, "Connect when the app starts");
    ui.checkbox(&mut state.develop_on_select, "Develop a photo as soon as it is selected");
}
