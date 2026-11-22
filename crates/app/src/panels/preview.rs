//! Preview: the selected photo, developed by the camera when one is
//! connected. It can show 1, 2 or 4 copies of the photo, each with its own
//! recipe, for comparing looks. Clicking a copy makes it the one that the
//! recipe picker and the Editor change.

use super::{hint, paint_fitted_with_border, View};
use crate::state::{AppState, Slot};
use eframe::egui::{self, Color32, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let Some(raf) = state.selected.clone() else {
        ui.centered_and_justified(|ui| hint(ui, "Select a photo in the Library."));
        return;
    };
    // While a crop is being adjusted, the editor takes the panel's place.
    if state.crop_edit.as_ref().is_some_and(|edit| edit.raf != raf) {
        state.crop_edit = None;
    }
    if state.crop_edit.is_some() {
        super::crop::show(ui, state);
        return;
    }
    header(ui, state);
    ui.separator();

    let (area, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
    ui.painter().rect_filled(area, 0.0, ui.visuals().extreme_bg_color);

    // The shape of the photo as shown, to choose the roomier arrangement.
    let orientation = state.orientation_of(&raf);
    let photo = match &state.embedded {
        Some((path, Slot::Ready(texture))) if path == &raf => orientation.shown_size(texture.size_vec2()),
        _ => Vec2::new(3.0, 2.0),
    };
    let cells = arrange(area, state.copies, photo);

    let mut clicked = None;
    let mut touch = Touch::default();
    for (index, cell) in cells.into_iter().enumerate() {
        let this = copy(ui, state, index, cell);
        if this.clicked {
            clicked = Some(index);
        }
        if this.zoom != 1.0 || this.pan != Vec2::ZERO || this.reset {
            touch = this;
        }
        if index == state.active_copy {
            state.fit_scale = this.fit.or(state.fit_scale);
        }
    }
    if let Some(index) = clicked {
        state.set_active_copy(index);
    }
    // Every copy shares one zoom and position, so they stay comparable.
    if touch.reset {
        state.zoom = 1.0;
    }
    state.zoom = (state.zoom * touch.zoom).clamp(1.0, 16.0);
    state.pan += touch.pan;
    if state.zoom <= 1.0 {
        state.pan = Vec2::ZERO;
    }
    state.pan = state.pan.clamp(Vec2::splat(-0.5), Vec2::splat(0.5));
}

/// What the mouse did on one copy this frame.
#[derive(Clone, Copy)]
struct Touch {
    clicked: bool,
    /// Multiply the zoom by this.
    zoom: f32,
    /// Add this to the position.
    pan: Vec2,
    /// Double-click: back to the whole photo.
    reset: bool,
    /// Screen points per pixel of the photo at zoom 1, if a photo was drawn.
    fit: Option<f32>,
}

impl Default for Touch {
    fn default() -> Self {
        Touch { clicked: false, zoom: 1.0, pan: Vec2::ZERO, reset: false, fit: None }
    }
}

/// Splits `area` into one cell per copy. Two copies go side by side or one
/// above the other, whichever shows a photo of this shape larger.
fn arrange(area: Rect, copies: usize, photo: Vec2) -> Vec<Rect> {
    const GAP: f32 = 4.0;
    let grid = |columns: usize, rows: usize| -> Vec<Rect> {
        let cell = Vec2::new(
            (area.width() - GAP * (columns as f32 - 1.0)) / columns as f32,
            (area.height() - GAP * (rows as f32 - 1.0)) / rows as f32,
        );
        (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (row, column)))
            .map(|(row, column)| {
                let min = area.min + Vec2::new(column as f32 * (cell.x + GAP), row as f32 * (cell.y + GAP));
                Rect::from_min_size(min, cell)
            })
            .collect()
    };
    // How large the photo comes out in a cell of this size.
    let scale = |cell: Vec2| (cell.x / photo.x).min(cell.y / photo.y);
    match copies {
        4 => grid(2, 2),
        2 => {
            let (beside, stacked) = (grid(2, 1), grid(1, 2));
            if scale(beside[0].size()) >= scale(stacked[0].size()) {
                beside
            } else {
                stacked
            }
        }
        _ => grid(1, 1),
    }
}

fn header(ui: &mut Ui, state: &mut AppState) {
    let Some(key) = state.current_key() else { return };
    let name = key.raf.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let developed = state.developed_for(&key).is_some();
    let developing = state.developing.contains(&key);

    // Wraps onto a second line when the panel is narrow.
    ui.horizontal_wrapped(|ui| {
        ui.strong(&name);
        ui.separator();

        ui.label("Copies");
        for count in [1usize, 2, 4] {
            let label = count.to_string();
            let hover = match count {
                1 => "Show the photo once.",
                2 => "Show two copies, each with its own recipe.",
                _ => "Show four copies, each with its own recipe.",
            };
            if ui.selectable_label(state.copies == count, label).on_hover_text(hover).clicked() {
                state.set_copies(count);
            }
        }
        ui.separator();

        let ready = state.camera.ready();
        if !developed && !developing {
            let develop = ui.add_enabled(ready, egui::Button::new("Develop"));
            if develop.on_disabled_hover_text("Connect the camera to develop.").clicked() {
                state.request_preview();
            }
        }
        if ui.button("⟲").on_hover_text("Turn a quarter turn to the left").clicked() {
            state.turn_selected(|o| o.rotated_ccw());
        }
        if ui.button("⟳").on_hover_text("Turn a quarter turn to the right").clicked() {
            state.turn_selected(|o| o.rotated_cw());
        }
        if ui.button("↔").on_hover_text("Flip left to right").clicked() {
            state.turn_selected(|o| o.flipped_horizontally());
        }
        if ui.button("↕").on_hover_text("Flip top to bottom").clicked() {
            state.turn_selected(|o| o.flipped_vertically());
        }
        let export_hint = if state.copies > 1 {
            "Export the highlighted copy at full size."
        } else {
            "Export the photo at full size."
        };
        let export = ui.add_enabled(ready, egui::Button::new("Export JPEG…"));
        if export.on_hover_text(export_hint).on_disabled_hover_text("Connect the camera to export.").clicked() {
            state.export_selected();
        }
        let cropped = state.crop_of(&key.raf).is_some();
        if ui
            .button(if cropped { "Crop ✔" } else { "Crop" })
            .on_hover_text("Crop and straighten. The RAF is not changed; previews and exports are cut to the frame.")
            .clicked()
        {
            state.crop_edit = Some(super::crop::CropEdit::start(key.raf.clone(), state.crop_of(&key.raf)));
            state.zoom = 1.0;
            state.pan = Vec2::ZERO;
        }
        ui.separator();
        if let Some(stars) = super::crop::stars_row(ui, state.stars_of(&key.raf)) {
            state.set_stars(&key.raf, stars);
        }
        ui.separator();
        if ui
            .add_enabled(state.zoom > 1.0, egui::Button::new("Fit"))
            .on_hover_text("Show the whole photo. Double-clicking the photo does the same.")
            .clicked()
        {
            state.zoom = 1.0;
            state.pan = Vec2::ZERO;
        }
        if ui
            .button("1:1")
            .on_hover_text("One pixel of the preview per screen pixel. Scroll over the photo to zoom, drag to move around.")
            .clicked()
        {
            if let Some(fit) = state.fit_scale {
                state.zoom = (1.0 / (fit * ui.ctx().pixels_per_point())).clamp(1.0, 16.0);
            }
        }
        if state.zoom > 1.0 {
            ui.label(egui::RichText::new(format!("×{:.1}", state.zoom)).weak());
        }
        if state.exporting > 0 {
            ui.spinner();
            ui.label("Exporting…");
        }
    });
}

/// Draws one copy of the photo in `cell`, with its label, and reports what
/// the mouse did there: click to highlight, wheel to zoom, drag to move
/// around, double-click to see the whole photo again.
fn copy(ui: &mut Ui, state: &AppState, index: usize, cell: Rect) -> Touch {
    let mut touch = Touch::default();
    let Some(key) = state.key_of(index) else { return touch };
    let orientation = state.orientation_of(&key.raf);
    let response = ui.interact(cell, ui.id().with(("copy", index)), Sense::click_and_drag());
    let picture = cell.shrink(6.0);
    let view = View { zoom: state.zoom, pan: state.pan };
    let crop = state.crop_of(&key.raf);
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            touch.zoom = (scroll / 240.0).exp();
        }
    }
    if response.dragged() && state.zoom > 1.0 {
        touch.pan = response.drag_delta() / (picture.size() * state.zoom);
    }
    touch.reset = response.double_clicked();

    // The camera's version if it has been made, otherwise the picture stored
    // in the file, which shows the original look.
    let developed = state.developed_for(&key);
    match developed {
        Some(texture) => touch.fit = paint_fitted_with_border(ui, picture, texture, orientation, crop, state.border, view),
        None => match &state.embedded {
            Some((path, Slot::Ready(texture))) if path == &key.raf => {
                touch.fit = paint_fitted_with_border(ui, picture, texture, orientation, crop, state.border, view)
            }
            Some((path, Slot::Missing)) if path == &key.raf => {
                ui.painter().text(
                    picture.center(),
                    egui::Align2::CENTER_CENTER,
                    "This file has no embedded preview.",
                    egui::FontId::proportional(14.0),
                    ui.visuals().weak_text_color(),
                );
            }
            _ => {}
        },
    }

    // The label: which recipe is on this copy, and what state it is in.
    let look = state.look_name_of(index);
    let error = state.develop_errors.iter().find(|(k, _)| k == &key).map(|(_, m)| m.as_str());
    let status = if developed.is_some() {
        None
    } else if state.developing.contains(&key) {
        Some("developing…")
    } else if error.is_some() {
        Some("failed")
    } else if key.recipe.is_some() {
        Some("not developed yet")
    } else {
        None
    };
    let text = match status {
        Some(status) => format!("{look} · {status}"),
        None => look,
    };
    label(ui, cell, &text, error.is_some());
    if let Some(message) = error {
        response.clone().on_hover_text(message);
    }

    // With several copies, mark the one that recipes apply to.
    if state.copies > 1 {
        let active = index == state.active_copy;
        let visuals = ui.visuals();
        let stroke = if active {
            // White shows best on a dark background; the accent on a light one.
            let colour = if visuals.dark_mode { visuals.selection.stroke.color } else { visuals.selection.bg_fill };
            Stroke::new(2.0, colour)
        } else if response.hovered() {
            Stroke::new(1.0, Color32::from_gray(120))
        } else {
            visuals.widgets.noninteractive.bg_stroke
        };
        ui.painter().rect_stroke(cell, 3.0, stroke, StrokeKind::Inside);
    }
    touch.clicked = response.clicked();
    touch
}

/// A small text plate in the lower left corner of a copy.
fn label(ui: &Ui, cell: Rect, text: &str, warning: bool) {
    let font = egui::FontId::proportional(12.0);
    let colour = if warning { ui.visuals().warn_fg_color } else { Color32::from_gray(235) };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, colour);
    let padding = Vec2::new(7.0, 3.0);
    let size = galley.size() + padding * 2.0;
    let min = egui::pos2(cell.left() + 10.0, cell.bottom() - 10.0 - size.y);
    let plate = Rect::from_min_size(min, size);
    ui.painter().rect_filled(plate, 4.0, Color32::from_black_alpha(190));
    ui.painter().galley(plate.min + padding, galley, colour);
}
