//! Editor: every setting of the current recipe as a control.
//!
//! Each setting has a tick box. Unticked means the recipe leaves that setting
//! alone, so the photo keeps what it was shot with.
//!
//! Ranges are the X-T4's menu ranges. A value another camera does not accept
//! is reported by the camera when the photo is developed.

use super::hint;
use crate::border::Border;
use crate::state::AppState;
use eframe::egui::{self, Ui};
use xrp_recipes::settings::*;
use super::pad;
use xrp_recipes::{apply, Recipe};

enum Action {
    Undo,
    Redo,
    /// Fill the sliders with the settings the selected photo was shot with.
    StartFromPhoto,
    SaveDescription,
    Revert,
    Save,
    SaveAsNew,
    RenderNow,
}

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let mut action = None;
    let mut changed = false;

    // Undo history belongs to one recipe being edited. If the settings were
    // changed from outside (another recipe or copy was chosen), start afresh.
    if state.editor.settings != state.editor.history_base {
        state.editor.undo.clear();
        state.editor.redo.clear();
        state.editor.last_edit = None;
    }
    let before = state.editor.settings.clone();
    // Ctrl+Z and Ctrl+Y (or Ctrl+Shift+Z), unless a text box is being typed in.
    if ui.ctx().memory(|m| m.focused().is_none()) {
        let (undo, redo) = ui.input_mut(|i| {
            let redo = i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z)
                || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y);
            (i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z), redo)
        });
        if undo {
            action = Some(Action::Undo);
        } else if redo {
            action = Some(Action::Redo);
        }
    }

    header(ui, state, &mut action);
    ui.separator();

    egui::ScrollArea::vertical().id_salt("editor_scroll").auto_shrink(false).show(ui, |ui| {
        changed = controls(ui, &mut state.editor.settings);
        border_controls(ui, &mut state.border);
        if let Some(text) = &state.editor.message {
            ui.add_space(4.0);
            hint(ui, text);
        }
    });

    if changed {
        state.editor.pending_since = Some(std::time::Instant::now());
    }
    let stepping = matches!(action, Some(Action::Undo | Action::Redo));
    if let Some(action) = action {
        perform(state, action);
    }
    // Remember the state before this change, so it can be undone. Changes
    // in quick succession (one slider drag) make a single step.
    if !stepping && state.editor.settings != before {
        let now = std::time::Instant::now();
        let same_gesture = changed
            && state.editor.last_edit.is_some_and(|t| now.duration_since(t) < std::time::Duration::from_millis(700));
        if !same_gesture {
            state.editor.undo.push(before);
            if state.editor.undo.len() > 100 {
                state.editor.undo.remove(0);
            }
        }
        state.editor.redo.clear();
        state.editor.last_edit = Some(now);
    }
    state.editor.history_base = state.editor.settings.clone();
}

fn header(ui: &mut Ui, state: &mut AppState, action: &mut Option<Action>) {
    let dirty = state.editor.dirty();
    let has_file = state.recipes.active.is_some();
    ui.horizontal_wrapped(|ui| {
        ui.strong(state.look_name());
        if ui
            .add_enabled(!state.editor.undo.is_empty(), egui::Button::new("Undo"))
            .on_hover_text("Take back the last change (Ctrl+Z).")
            .clicked()
        {
            *action = Some(Action::Undo);
        }
        if ui
            .add_enabled(!state.editor.redo.is_empty(), egui::Button::new("Redo"))
            .on_hover_text("Put back what was undone (Ctrl+Y).")
            .clicked()
        {
            *action = Some(Action::Redo);
        }
        if ui.add_enabled(dirty, egui::Button::new("Revert")).on_hover_text("Undo all changes since the last save.").clicked() {
            *action = Some(Action::Revert);
        }
        if ui
            .add_enabled(dirty && has_file, egui::Button::new("Save"))
            .on_hover_text("Write the changes into this recipe's file.")
            .clicked()
        {
            *action = Some(Action::Save);
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.editor.name)
                .hint_text("Name for a new recipe")
                .desired_width(150.0),
        );
        let can = !state.editor.name.trim().is_empty() && !state.editor.settings.is_empty();
        if ui
            .add_enabled(can, egui::Button::new("Save as new"))
            .on_hover_text("Create a new recipe file with these settings.")
            .clicked()
        {
            *action = Some(Action::SaveAsNew);
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.editor.auto_render, "Auto-render")
            .on_hover_text("Develop a new preview shortly after each change.");
        if !state.editor.auto_render && ui.button("Render").clicked() {
            *action = Some(Action::RenderNow);
        }
        let have_look = photo_look(state).is_some();
        if ui
            .add_enabled(have_look, egui::Button::new("Start from this photo"))
            .on_hover_text("Fill in the settings the selected photo was shot with, as a new recipe.")
            .on_disabled_hover_text("Select a photo first.")
            .clicked()
        {
            *action = Some(Action::StartFromPhoto);
        }
    });
    description(ui, state, action);
}

/// The active recipe's description, stored inside its file, and anything the
/// camera reported when it last developed with this recipe.
fn description(ui: &mut Ui, state: &mut AppState, action: &mut Option<Action>) {
    let Some(file_name) = state.recipes.active.clone() else { return };
    let stored = state
        .recipes
        .library
        .as_ref()
        .ok()
        .and_then(|l| l.find(&file_name))
        .and_then(|e| e.recipe.as_ref().ok())
        .map(|r| r.description.clone());
    let Some(stored) = stored else { return };

    let editing = matches!(&state.recipes.description_draft, Some((f, _)) if f == &file_name);
    if !editing {
        state.recipes.description_draft = Some((file_name.clone(), stored.clone()));
    }
    if let Some((_, text)) = &mut state.recipes.description_draft {
        ui.add(
            egui::TextEdit::multiline(text)
                .hint_text("Description (stored in the recipe file, shown when hovering)")
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        if text.trim() != stored.trim() && ui.button("Save description").clicked() {
            *action = Some(Action::SaveDescription);
        }
    }
    if let Some(notes) = state.recipes.notes.get(&file_name) {
        for note in notes {
            ui.colored_label(ui.visuals().warn_fg_color, note);
        }
    }
}

/// One row: a tick box that turns the setting on, then its control.
/// Returns true if anything changed.
fn row<T: Clone>(
    ui: &mut Ui,
    label: &str,
    value: &mut Option<T>,
    default: T,
    control: impl FnOnce(&mut Ui, &mut T) -> bool,
) -> bool {
    let mut changed = false;
    let mut on = value.is_some();
    if ui.checkbox(&mut on, label).changed() {
        *value = if on { Some(default) } else { None };
        changed = true;
    }
    match value {
        Some(v) => changed |= control(ui, v),
        None => hint(ui, "as shot"),
    }
    ui.end_row();
    changed
}

/// A row with a colour pad for two linked values, and a number box for each.
/// One tick box turns both on. `scale` is how the stored values relate to
/// pad steps (10 for values kept in tenths).
#[allow(clippy::too_many_arguments)]
fn pad_row(
    ui: &mut Ui,
    label: &str,
    names: (&str, &str),
    across: &mut Option<i32>,
    up: &mut Option<i32>,
    range: i32,
    scale: i32,
    corners: &pad::Corners,
) -> bool {
    let mut changed = false;
    let mut on = across.is_some() || up.is_some();
    if ui.checkbox(&mut on, label).changed() {
        *across = on.then_some(0);
        *up = on.then_some(0);
        changed = true;
    }
    if on {
        let (mut x, mut y) = (across.unwrap_or(0) / scale, up.unwrap_or(0) / scale);
        ui.vertical(|ui| {
            changed |= pad::show(ui, &mut x, &mut y, range, corners, 132.0);
            ui.horizontal(|ui| {
                let signed = |v: f64, _: std::ops::RangeInclusive<usize>| if v == 0.0 { "0".to_string() } else { format!("{v:+.0}") };
                ui.label(names.0);
                changed |= ui.add(egui::DragValue::new(&mut x).range(-range..=range).speed(0.1).custom_formatter(signed)).changed();
                ui.label(names.1);
                changed |= ui.add(egui::DragValue::new(&mut y).range(-range..=range).speed(0.1).custom_formatter(signed)).changed();
            });
        });
        *across = Some(x * scale);
        *up = Some(y * scale);
    } else {
        hint(ui, "as shot");
    }
    ui.end_row();
    changed
}

/// The look of the selected photo: from the camera if it has loaded this
/// photo (most exact), otherwise as recorded in the file.
fn photo_look(state: &AppState) -> Option<Settings> {
    let selected = state.selected.as_ref()?;
    if let Some((raf, profile)) = &state.as_shot {
        if raf == selected {
            // Go through the file format so the editor shows exactly what would be saved.
            let text = Recipe::build("", "", "", &apply::fields_from_profile(profile));
            return Recipe::parse(&text).ok().map(|r| Settings::from_recipe(&r));
        }
    }
    state.info.as_ref().and_then(|i| i.look.clone())
}

/// A slider over tenths (15 = +1.5) that shows menu values.
fn tone_slider(ui: &mut Ui, value: &mut i32, range: std::ops::RangeInclusive<i32>, step: i32) -> bool {
    ui.add(
        egui::Slider::new(value, range)
            .step_by(step as f64)
            .custom_formatter(|v, _| {
                let v = v / 10.0;
                if v == 0.0 {
                    "0".into()
                } else if v.fract() == 0.0 {
                    format!("{v:+.0}")
                } else {
                    format!("{v:+.1}")
                }
            }),
    )
    .changed()
}

fn whole_slider(ui: &mut Ui, value: &mut i32, range: std::ops::RangeInclusive<i32>) -> bool {
    ui.add(egui::Slider::new(value, range).custom_formatter(|v, _| if v == 0.0 { "0".into() } else { format!("{v:+.0}") }))
        .changed()
}

fn level_combo(ui: &mut Ui, id: &str, value: &mut Level) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).selected_text(value.label()).show_ui(ui, |ui| {
        for level in [Level::Off, Level::Weak, Level::Strong] {
            changed |= ui.selectable_value(value, level, level.label()).changed();
        }
    });
    changed
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(title).strong().weak());
}

fn grid<R>(ui: &mut Ui, id: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Grid::new(id).num_columns(2).spacing([10.0, 5.0]).min_col_width(118.0).show(ui, body).inner
}

/// All the controls. Returns true if any setting changed.
fn controls(ui: &mut Ui, s: &mut Settings) -> bool {
    let mut changed = false;

    section(ui, "Look");
    changed |= grid(ui, "editor_look", |ui| {
        let mut c = false;
        c |= row(ui, "Film simulation", &mut s.film_simulation, 0x01, |ui, code| {
            let mut c = false;
            egui::ComboBox::from_id_salt("film_simulation")
                .selected_text(film_simulation_name(*code).unwrap_or("?"))
                .height(420.0)
                .show_ui(ui, |ui| {
                    for (value, _, name) in FILM_SIMULATIONS {
                        c |= ui.selectable_value(code, *value, *name).changed();
                    }
                });
            c
        });
        c |= row(ui, "Grain", &mut s.grain, Grain { strength: Level::Weak, large: false }, |ui, g| {
            let mut c = level_combo(ui, "grain", &mut g.strength);
            if g.strength != Level::Off {
                c |= ui.checkbox(&mut g.large, "Large").changed();
            }
            c
        });
        c |= row(ui, "Color Chrome", &mut s.color_chrome, Level::Weak, |ui, l| level_combo(ui, "chrome", l));
        c |= row(ui, "Color Chrome FX Blue", &mut s.color_chrome_blue, Level::Weak, |ui, l| {
            level_combo(ui, "chrome_blue", l)
        });
        c
    });

    section(ui, "White balance");
    changed |= grid(ui, "editor_wb", |ui| {
        let mut c = false;
        c |= row(ui, "White balance", &mut s.white_balance, WhiteBalance::Mode(0x0002), |ui, wb| {
            let mut c = false;
            let label = match wb {
                WhiteBalance::AsShot => "As shot".to_string(),
                WhiteBalance::Kelvin(_) => "Colour temperature".to_string(),
                WhiteBalance::Mode(code) => WHITE_BALANCES
                    .iter()
                    .find(|(m, _, _)| m == code)
                    .map(|(_, _, name)| name.to_string())
                    .unwrap_or_else(|| "?".into()),
            };
            egui::ComboBox::from_id_salt("white_balance").selected_text(label).height(360.0).show_ui(ui, |ui| {
                c |= ui.selectable_value(wb, WhiteBalance::AsShot, "As shot").changed();
                for (code, _, name) in WHITE_BALANCES {
                    if *code == WB_TEMPERATURE {
                        let is_kelvin = matches!(wb, WhiteBalance::Kelvin(_));
                        if ui.selectable_label(is_kelvin, *name).clicked() && !is_kelvin {
                            *wb = WhiteBalance::Kelvin(5500);
                            c = true;
                        }
                    } else {
                        c |= ui.selectable_value(wb, WhiteBalance::Mode(*code), *name).changed();
                    }
                }
            });
            if let WhiteBalance::Kelvin(k) = wb {
                c |= ui.add(egui::DragValue::new(k).range(2500..=10000).speed(10).suffix(" K")).changed();
            }
            c
        });
        c |= pad_row(ui, "WB shift", ("R", "B"), &mut s.wb_shift_red, &mut s.wb_shift_blue, 9, 1, &pad::WB_SHIFT);
        c
    });

    section(ui, "Tone");
    changed |= grid(ui, "editor_tone", |ui| {
        let mut c = false;
        c |= row(ui, "Dynamic range", &mut s.dynamic_range, 100, |ui, dr| {
            let mut c = false;
            egui::ComboBox::from_id_salt("dynamic_range").selected_text(format!("DR{dr}")).show_ui(ui, |ui| {
                for value in [100, 200, 400] {
                    c |= ui.selectable_value(dr, value, format!("DR{value}")).changed();
                }
            });
            c
        });
        c |= row(ui, "Highlight", &mut s.highlight, 0, |ui, v| tone_slider(ui, v, -20..=40, 5));
        c |= row(ui, "Shadow", &mut s.shadow, 0, |ui, v| tone_slider(ui, v, -20..=40, 5));
        c |= row(ui, "Exposure", &mut s.exposure_thirds, 0, |ui, v| {
            ui.add(egui::Slider::new(v, -9..=9).custom_formatter(|v, _| exposure_label(v as i32))).changed()
        });
        c
    });

    let mono = s.film_simulation.map(is_monochrome).unwrap_or(false);
    section(ui, "Colour and detail");
    changed |= grid(ui, "editor_detail", |ui| {
        let mut c = false;
        if mono {
            // Stored in tenths; the pad and boxes work in whole steps, −18..+18.
            // Across is magenta to green, up is warm.
            c |= pad_row(ui, "Mono colour", ("MG", "WC"), &mut s.mono_magenta_green, &mut s.mono_warm_cool, 18, 10, &pad::MONO_COLOUR);
        } else {
            c |= row(ui, "Color", &mut s.color, 0, |ui, v| tone_slider(ui, v, -40..=40, 10));
        }
        c |= row(ui, "Sharpness", &mut s.sharpness, 0, |ui, v| tone_slider(ui, v, -40..=40, 10));
        c |= row(ui, "Noise reduction", &mut s.noise_reduction, 0, |ui, v| whole_slider(ui, v, -4..=4));
        c |= row(ui, "Clarity", &mut s.clarity, 0, |ui, v| tone_slider(ui, v, -50..=50, 10));
        c
    });
    changed
}

/// The white border. Not part of the recipe: it applies to whatever photo
/// and recipe are shown, and to exports, until it is changed.
fn border_controls(ui: &mut Ui, border: &mut Border) {
    section(ui, "Finish");
    grid(ui, "editor_finish", |ui| {
        ui.checkbox(&mut border.on, "Border")
            .on_hover_text("Adds a border around the photo, in the preview and in exported JPEGs. Not saved in the recipe.");
        ui.add_enabled_ui(border.on, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().slider_width = 110.0;
                ui.add(egui::Slider::new(&mut border.percent, 0.0..=Border::MAX_PERCENT).step_by(0.5).show_value(false));
                ui.add(
                    egui::DragValue::new(&mut border.percent)
                        .range(0.0..=Border::MAX_PERCENT)
                        .speed(0.1)
                        .max_decimals(1)
                        .suffix(" %"),
                )
                .on_hover_text("Border width as a percentage of the photo's shorter side. Click to type a number.");
            });
        });
        ui.end_row();

        ui.add_enabled(border.on, egui::Label::new("Wider bottom edge"));
        ui.add_enabled_ui(border.on, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().slider_width = 110.0;
                ui.add(egui::Slider::new(&mut border.bottom_extra, 0.0..=Border::MAX_PERCENT).step_by(0.5).show_value(false));
                ui.add(
                    egui::DragValue::new(&mut border.bottom_extra)
                        .range(0.0..=Border::MAX_PERCENT)
                        .speed(0.1)
                        .max_decimals(1)
                        .suffix(" %"),
                )
                .on_hover_text("Extra width on the bottom edge only, for a print look.");
            });
        });
        ui.end_row();

        ui.add_enabled(border.on, egui::Label::new("Border colour"));
        ui.add_enabled_ui(border.on, |ui| {
            ui.horizontal(|ui| {
                ui.color_edit_button_srgb(&mut border.color);
                for (name, colour) in [("White", [255, 255, 255]), ("Black", [0, 0, 0]), ("Grey", [128, 128, 128])] {
                    if ui.small_button(name).clicked() {
                        border.color = colour;
                    }
                }
            });
        });
        ui.end_row();
    });
    ui.add_space(6.0);
}

fn perform(state: &mut AppState, action: Action) {
    let message = match action {
        Action::RenderNow => {
            state.request_preview();
            return;
        }
        Action::StartFromPhoto => match (photo_look(state), state.selected.clone()) {
            (Some(settings), Some(raf)) => {
                let photo = raf.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                state.recipes.active = None;
                state.recipes.description_draft = None;
                state.editor.name = format!("{photo} look");
                state.editor.saved = Settings::default();
                state.editor.settings = settings;
                state.editor.pending_since = Some(std::time::Instant::now());
                Some("Filled in this photo's settings. Adjust them, then press Save as new.".to_string())
            }
            _ => Some("Select a photo first.".to_string()),
        },
        Action::SaveDescription => {
            let Some((file_name, text)) = state.recipes.description_draft.clone() else { return };
            match &mut state.recipes.library {
                Ok(library) => match library.set_description(&file_name, &text) {
                    Ok(()) => {
                        state.recipes.description_draft = None;
                        Some("Description saved in the recipe file.".to_string())
                    }
                    Err(problem) => Some(problem),
                },
                Err(problem) => Some(problem.clone()),
            }
        }
        Action::Undo => {
            let Some(earlier) = state.editor.undo.pop() else { return };
            let current = std::mem::replace(&mut state.editor.settings, earlier);
            state.editor.redo.push(current);
            state.editor.last_edit = None;
            state.editor.pending_since = Some(std::time::Instant::now());
            None
        }
        Action::Redo => {
            let Some(later) = state.editor.redo.pop() else { return };
            let current = std::mem::replace(&mut state.editor.settings, later);
            state.editor.undo.push(current);
            state.editor.last_edit = None;
            state.editor.pending_since = Some(std::time::Instant::now());
            None
        }
        Action::Revert => {
            state.editor.settings = state.editor.saved.clone();
            state.editor.pending_since = Some(std::time::Instant::now());
            None
        }
        Action::Save => {
            let Some(file_name) = state.recipes.active.clone() else { return };
            let settings = state.editor.settings.clone();
            match &mut state.recipes.library {
                Ok(library) => match library.save_settings(&file_name, &settings) {
                    Ok(()) => {
                        state.editor.saved = settings;
                        Some(format!("Saved {file_name}."))
                    }
                    Err(problem) => Some(problem),
                },
                Err(problem) => Some(problem.clone()),
            }
        }
        Action::SaveAsNew => {
            let name = state.editor.name.trim().to_string();
            let settings = state.editor.settings.clone();
            let device = match &state.camera {
                crate::state::CameraStatus::Connected { model, .. } => model.clone(),
                _ => String::new(),
            };
            let text = Recipe::build(&name, &device, "", &settings.to_fields());
            match &mut state.recipes.library {
                Ok(library) => match library.create(&name, &text) {
                    Ok(file_name) => {
                        state.editor.saved = settings;
                        state.recipes.active = Some(file_name.clone());
                        state.recipes.description_draft = None;
                        Some(format!("Saved {file_name}."))
                    }
                    Err(problem) => Some(problem),
                },
                Err(problem) => Some(problem.clone()),
            }
        }
    };
    if let Some(text) = message {
        state.log.push(text.clone());
        state.editor.message = Some(text);
    }
}
