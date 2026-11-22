//! Camera Slots panel: the camera's custom settings (C1, C2, …), uploading
//! the current recipe into one, and saving a slot into the recipe library.

use super::hint;
use crate::camera_worker::{CameraCommand, RecipeJob};
use crate::slots::{SlotInfo, Upload, BACKUP_CATEGORY};
use crate::state::AppState;
use eframe::egui::{self, RichText, Ui};
use xrp_camera::slots::{clean_name, WriteOutcome, NAME_LIMIT};
use xrp_recipes::{slot, Library, Recipe};

enum Action {
    Read,
    /// Ask to upload the current recipe to this slot.
    AskUpload(u16),
    /// Save this slot's look as a recipe in the library.
    SaveToLibrary(u16),
    /// Ask to put a backup (recipe file name) back into this slot.
    AskRestore(u16, String),
    ConfirmUpload,
    CancelUpload,
}

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let mut action = None;

    if !state.camera.ready() {
        // What was read belongs to a camera that is no longer there.
        state.slots.list = None;
        state.slots.error = None;
        state.slots.busy = false;
        state.slots.pending = None;
        hint(ui, "Connect the camera (USB RAW CONV./BACKUP RESTORE mode) to see its custom slots.");
        last_report(ui, state);
        return;
    }
    // Read the slots the first time the panel is shown with a camera.
    if state.slots.list.is_none() && state.slots.error.is_none() && !state.slots.busy {
        action = Some(Action::Read);
    }

    ui.horizontal(|ui| {
        if ui.add_enabled(!state.slots.busy, egui::Button::new("Read slots").small()).clicked() {
            action = Some(Action::Read);
        }
        if state.slots.busy {
            ui.spinner();
            ui.label("Talking to the camera…");
        }
    });
    let job = state.active_job();
    match &job {
        Some(job) => {
            ui.horizontal_wrapped(|ui| {
                ui.label("Recipe to upload:");
                ui.label(RichText::new(&job.name).strong());
            });
        }
        None => hint(ui, "Click a recipe in the Recipes list, then press Upload on a slot."),
    }
    ui.separator();

    if let Some(problem) = &state.slots.error {
        ui.colored_label(ui.visuals().warn_fg_color, problem);
    }
    // The strip this panel sits in scrolls as a whole.
    ui.vertical(|ui| {
        if let Some(slots) = &state.slots.list {
            let library = state.recipes.library.as_ref().ok();
            for info in slots {
                let same_as = library.and_then(|l| matching_recipe(info, l));
                let backups = library.map(|l| backups_of(info.read.number, l)).unwrap_or_default();
                let free = !state.slots.busy;
                slot_row(ui, info, job.is_some() && free, same_as, &backups, free, &mut action);
            }
        }
        if let Some(text) = &state.slots.message {
            ui.add_space(4.0);
            hint(ui, text);
        }
        last_report(ui, state);
    });

    confirm_window(ui, state, &mut action);
    if let Some(action) = action {
        perform(state, action);
    }
}

/// The library recipe whose settings this slot already holds, if any: one
/// that uploading would change nothing for. Backups are not considered.
fn matching_recipe(info: &SlotInfo, library: &Library) -> Option<String> {
    library
        .recipes
        .iter()
        .filter(|entry| !library.is_in(BACKUP_CATEGORY, &entry.file_name))
        .find(|entry| {
            let plan = slot::plan(&entry.settings, &info.read);
            !plan.stored.is_empty()
                && plan.writes.iter().all(|(code, wanted)| {
                    let outcome =
                        WriteOutcome { prop: *code, wanted: *wanted, response: 0x2001, read_back: info.read.get(*code) };
                    slot::took(&outcome)
                })
        })
        .map(|entry| entry.display_name())
}

/// Backups made of this slot before uploads: (file name, shown name).
fn backups_of(number: u16, library: &Library) -> Vec<(String, String)> {
    let prefix = format!("C{number} backup");
    library
        .recipes_in(BACKUP_CATEGORY)
        .into_iter()
        .filter(|entry| entry.file_name.starts_with(&prefix))
        .map(|entry| (entry.file_name.clone(), entry.file_name.trim_end_matches(".FP1").to_string()))
        .collect()
}

/// One slot: its number, name and look, with its buttons.
fn slot_row(
    ui: &mut Ui,
    info: &SlotInfo,
    can_upload: bool,
    same_as: Option<String>,
    backups: &[(String, String)],
    free: bool,
    action: &mut Option<Action>,
) {
    let number = info.read.number;
    let film = info.settings.film_simulation.and_then(xrp_recipes::settings::film_simulation_name);
    // Two short lines per slot, so the panel fits a narrow strip: the
    // slot's number and name with "Upload"; under it, what it holds, with
    // the rarer actions in a menu.
    ui.horizontal(|ui| {
        ui.label(RichText::new(info.label()).strong().monospace());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(can_upload, egui::Button::new("Upload").small())
                .on_hover_text("Store the current recipe and its name in this slot. You are asked to confirm first.")
                .on_disabled_hover_text("Pick a recipe first.")
                .clicked()
            {
                *action = Some(Action::AskUpload(number));
            }
            // The name takes the space that is left, cut short if need be.
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let name = match info.read.name.trim() {
                    "" => RichText::new("(no name)").weak(),
                    name => RichText::new(name),
                };
                ui.add(egui::Label::new(name).truncate()).on_hover_ui(|ui| look_card(ui, info));
            });
        });
    });
    ui.horizontal(|ui| {
        ui.add_space(26.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(free, |ui| {
                ui.menu_button(RichText::new("More").small(), |ui| {
                    if ui.button("Save to library").on_hover_text("Save what this slot holds as a recipe file.").clicked() {
                        *action = Some(Action::SaveToLibrary(number));
                        ui.close();
                    }
                    if !backups.is_empty() {
                        ui.separator();
                        ui.label(RichText::new("Put back what this slot held before:").weak());
                        for (file_name, shown) in backups {
                            if ui.button(shown).clicked() {
                                *action = Some(Action::AskRestore(number, file_name.clone()));
                                ui.close();
                            }
                        }
                    }
                });
            });
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let holds = match (film, &same_as) {
                    (_, Some(recipe)) => format!("= {recipe}"),
                    (Some(film), None) => film.to_string(),
                    (None, None) => String::new(),
                };
                ui.add(egui::Label::new(RichText::new(holds).weak().small()).truncate()).on_hover_ui(|ui| look_card(ui, info));
            });
        });
    });
    ui.add_space(2.0);
}

/// Everything a slot's look holds, shown when the mouse is over it.
fn look_card(ui: &mut Ui, info: &SlotInfo) {
    ui.set_max_width(320.0);
    ui.label(RichText::new(format!("{} {}", info.label(), info.read.name.trim())).strong());
    egui::Grid::new(("slot_look", info.read.number)).num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
        for (name, value) in info.settings.summary() {
            ui.label(RichText::new(name).weak());
            ui.label(value);
            ui.end_row();
        }
    });
}

fn last_report(ui: &mut Ui, state: &AppState) {
    let Some(report) = &state.slots.report else { return };
    ui.add_space(6.0);
    ui.separator();
    let title = if report.complete { "Last upload: done" } else { "Last upload: check these" };
    ui.label(RichText::new(title).strong());
    for line in &report.lines {
        ui.label(line);
    }
}

/// The question asked before a slot is overwritten.
fn confirm_window(ui: &mut Ui, state: &mut AppState, action: &mut Option<Action>) {
    let Some(upload) = &mut state.slots.pending else { return };
    let Some(info) = state.slots.list.as_ref().and_then(|l| l.iter().find(|i| i.read.number == upload.number))
    else {
        return;
    };
    let plan = slot::plan(&upload.job.settings, &info.read);
    let mut open = true;
    egui::Window::new("Upload recipe to camera")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ui.ctx(), |ui| {
            ui.set_width(420.0);
            let old = match info.read.name.trim() {
                "" => "no name".to_string(),
                name => format!("\"{name}\""),
            };
            ui.label(format!(
                "Store \"{}\" in slot {}? It now holds {}, which will be replaced.",
                upload.job.name,
                info.label(),
                old
            ));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Name in the camera:");
                ui.add(egui::TextEdit::singleline(&mut upload.name).char_limit(NAME_LIMIT).desired_width(220.0));
            });
            let cleaned = clean_name(&upload.name);
            if cleaned != upload.name.trim() {
                hint(ui, &format!("The camera only shows plain letters, digits and signs: it will be \"{cleaned}\"."));
            } else {
                hint(ui, &format!("Up to {NAME_LIMIT} plain characters."));
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Will be stored").strong());
            ui.label(if plan.stored.is_empty() { "Nothing: this recipe sets nothing a slot holds.".to_string() } else { plan.stored.join(", ") });
            if !plan.kept.is_empty() {
                ui.label(RichText::new("Not set by this recipe (the slot keeps its own)").strong());
                ui.label(plan.kept.join(", "));
            }
            for note in &plan.notes {
                ui.colored_label(ui.visuals().warn_fg_color, note);
            }
            ui.add_space(6.0);
            hint(
                ui,
                &format!(
                    "What the slot holds now is first saved to the Recipes folder, in the category \"{BACKUP_CATEGORY}\". \
                     Image size, quality and other camera settings in the slot are not touched."
                ),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!cleaned.is_empty(), egui::Button::new("Upload")).clicked() {
                    *action = Some(Action::ConfirmUpload);
                }
                if ui.button("Cancel").clicked() {
                    *action = Some(Action::CancelUpload);
                }
            });
        });
    if !open && action.is_none() {
        *action = Some(Action::CancelUpload);
    }
}

fn perform(state: &mut AppState, action: Action) {
    match action {
        Action::Read => {
            state.slots.busy = true;
            state.slots.error = None;
            state.send_camera(CameraCommand::ReadSlots);
        }
        Action::AskUpload(number) => {
            if let Some(job) = state.active_job() {
                state.slots.pending = Some(Upload { number, name: clean_name(&job.name), job });
            }
        }
        Action::AskRestore(number, file_name) => {
            let Ok(library) = &state.recipes.library else { return };
            let Some(entry) = library.find(&file_name) else { return };
            // The backup's own name is the name the slot had.
            let name = entry.recipe.as_ref().map(|r| r.name.clone()).unwrap_or_else(|_| entry.display_name());
            let job = RecipeJob { file_name: entry.file_name.clone(), name: name.clone(), settings: entry.settings.clone() };
            state.slots.pending = Some(Upload { number, name: clean_name(&name), job });
        }
        Action::CancelUpload => state.slots.pending = None,
        Action::ConfirmUpload => {
            let Some(upload) = state.slots.pending.take() else { return };
            let folder = match &state.recipes.library {
                Ok(library) => library.folder.clone(),
                Err(problem) => {
                    // No folder to keep the backup in, so do not overwrite.
                    state.slots.message = Some(format!("Not uploaded: the Recipes folder is not available. {problem}"));
                    return;
                }
            };
            state.slots.busy = true;
            state.slots.message = None;
            state.log(format!("Uploading \"{}\" to C{}…", upload.job.name, upload.number));
            state.send_camera(CameraCommand::WriteSlot {
                number: upload.number,
                name: upload.name,
                job: upload.job,
                backup_folder: folder,
            });
        }
        Action::SaveToLibrary(number) => {
            let Some(info) = state.slots.list.as_ref().and_then(|l| l.iter().find(|i| i.read.number == number)).cloned()
            else {
                return;
            };
            let name = match info.read.name.trim() {
                "" => info.label(),
                name => name.to_string(),
            };
            let device = match &state.camera {
                crate::state::CameraStatus::Connected { model, .. } => model.clone(),
                _ => String::new(),
            };
            let description = format!("Read from camera slot {}.", info.label());
            let text = Recipe::build(&name, &device, &description, &info.settings.to_fields());
            let message = match &mut state.recipes.library {
                Ok(library) => match library.create(&name, &text) {
                    Ok(file_name) => format!("Saved {} as {file_name}.", info.label()),
                    Err(problem) => problem,
                },
                Err(problem) => problem.clone(),
            };
            state.log(message.clone());
            state.slots.message = Some(message);
        }
    }
}
