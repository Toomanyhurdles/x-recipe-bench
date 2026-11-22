//! Library: the RAF files in the open folder, as a grid of thumbnails.

use super::{hint, paint_fitted, GAP};
use crate::events::EmbeddedKind;
use crate::image_worker::ImageRequest;
use crate::state::{AppState, Slot};
use eframe::egui::{self, Color32, Sense, Stroke, StrokeKind, Ui, Vec2};

/// The smallest and largest thumbnail width the size slider offers.
pub const THUMBNAIL_SIZES: (f32, f32) = (96.0, 320.0);
const LABEL_HEIGHT: f32 = 18.0;
/// Distance from one star to the next on a thumbnail.
const STAR_STEP: f32 = 16.0;

pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        if ui.button("Open folder…").clicked() {
            state.pick_folder();
        }
        ui.add(
            egui::Slider::new(&mut state.thumbnail_size, THUMBNAIL_SIZES.0..=THUMBNAIL_SIZES.1)
                .show_value(false)
                .text("Size"),
        );
    });

    let Some(folder) = state.folder.clone() else {
        ui.add_space(12.0);
        hint(ui, "Open a folder of RAF files to start.");
        return;
    };
    let files = state.visible_files();
    let count = if files.len() == state.files.len() {
        format!("{} files", state.files.len())
    } else {
        format!("{} of {} files shown", files.len(), state.files.len())
    };
    hint(ui, &format!("{}  ·  {count}  ·  stars: click them on a thumbnail, right-click it, or press 1–5", folder.display()));
    shutter_range(ui, state);
    filter_bar(ui, state);
    ui.separator();

    if state.files.is_empty() {
        hint(ui, "No RAF files in this folder.");
        return;
    }
    if files.is_empty() {
        hint(ui, "No photo matches the filters.");
        return;
    }

    let cell = Vec2::new(state.thumbnail_size, state.thumbnail_size * 0.72 + LABEL_HEIGHT);
    // Count the columns that fit in what can be seen. The layout can think
    // it is wider than the panel (a long line above widens it), and then a
    // column would be drawn out of sight and its photos missed.
    let visible = (ui.clip_rect().right().min(ui.max_rect().right()) - ui.cursor().left() - 12.0).max(cell.x);
    let columns = (((visible + GAP.x) / (cell.x + GAP.x)).floor() as usize).max(1);
    let rows = files.len().div_ceil(columns);
    let mut clicked = None;

    egui::ScrollArea::vertical().auto_shrink(false).show_rows(
        ui,
        cell.y + GAP.y,
        rows,
        |ui, row_range| {
            ui.spacing_mut().item_spacing = GAP;
            for row in row_range {
                ui.horizontal(|ui| {
                    for index in row * columns..((row + 1) * columns).min(files.len()) {
                        match thumbnail(ui, state, &files[index], cell) {
                            Click::Double => clicked = Some((index, true)),
                            Click::Single => clicked = clicked.or(Some((index, false))),
                            Click::None => {}
                        }
                    }
                });
            }
        },
    );

    if let Some((index, open)) = clicked {
        let path = files[index].clone();
        state.select(&path);
        let _ = open;
    }
}

enum Click {
    None,
    Single,
    Double,
}

/// Draws one cell and reports whether it was clicked.
fn thumbnail(ui: &mut Ui, state: &mut AppState, path: &std::path::Path, cell: Vec2) -> Click {
    let path = path.to_path_buf();
    let selected = state.selected.as_ref() == Some(&path);
    let (rect, response) = ui.allocate_exact_size(cell, Sense::click());
    if !ui.is_rect_visible(rect) {
        return Click::None;
    }
    let orientation = state.orientation_of(&path);

    // Ask for the thumbnail the first time the cell is seen.
    if !state.thumbnails.contains_key(&path) {
        state.thumbnails.insert(path.clone(), Slot::Loading);
        let _ = state
            .image_tx
            .send(ImageRequest { raf: path.clone(), kind: EmbeddedKind::Thumbnail });
    }

    let visuals = ui.visuals().clone();
    let background = if selected {
        visuals.selection.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        visuals.extreme_bg_color
    };
    ui.painter().rect_filled(rect, 4.0, background);

    let mut picture = rect.shrink(4.0);
    picture.max.y -= LABEL_HEIGHT;
    match state.thumbnails.get(&path) {
        Some(Slot::Ready(texture)) => paint_fitted(ui, picture, texture, orientation, state.crop_of(&path)),
        Some(Slot::Missing) => {
            ui.painter().text(
                picture.center(),
                egui::Align2::CENTER_CENTER,
                "no preview",
                egui::FontId::proportional(11.0),
                visuals.weak_text_color(),
            );
        }
        _ => {}
    }

    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let text_color = if selected { visuals.selection.stroke.color } else { visuals.text_color() };
    ui.painter().text(
        egui::pos2(rect.center().x, rect.max.y - LABEL_HEIGHT / 2.0 - 2.0),
        egui::Align2::CENTER_CENTER,
        name,
        egui::FontId::proportional(12.0),
        text_color,
    );
    // Five stars in the corner of every thumbnail. The photo's stars are
    // gold; the rest appear as outlines while the mouse is over the
    // thumbnail (or it is selected), ready to be clicked.
    let stars = state.stars_of(&path);
    let strip = egui::Rect::from_min_size(rect.left_top() + Vec2::new(6.0, 4.0), Vec2::new(STAR_STEP * 5.0, 18.0));
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let over_star = pointer.filter(|p| strip.contains(*p)).map(|p| ((p.x - strip.left()) / STAR_STEP) as u8 + 1);
    if stars > 0 || response.hovered() || selected {
        for n in 1..=5u8 {
            let filled = n <= stars;
            // Stars up to the one under the mouse light up as a preview.
            let lit = over_star.is_some_and(|over| n <= over) && response.hovered();
            if !filled && !lit && !(response.hovered() || selected) {
                continue;
            }
            let sign = if filled || lit { "★" } else { "☆" };
            let colour = if filled {
                Color32::from_rgb(255, 205, 70)
            } else if lit {
                Color32::from_rgb(255, 235, 170)
            } else {
                Color32::from_gray(230)
            };
            let at = strip.left_top() + Vec2::new((n - 1) as f32 * STAR_STEP, 0.0);
            // A dark copy underneath keeps the stars readable on any photo.
            ui.painter().text(at + Vec2::splat(1.0), egui::Align2::LEFT_TOP, sign, egui::FontId::proportional(15.0), Color32::from_black_alpha(210));
            ui.painter().text(at, egui::Align2::LEFT_TOP, sign, egui::FontId::proportional(15.0), colour);
        }
    }
    // A click on a star sets that many; on the last gold star, removes them.
    if response.clicked() {
        if let Some(n) = response.interact_pointer_pos().filter(|p| strip.contains(*p)).map(|p| ((p.x - strip.left()) / STAR_STEP) as u8 + 1) {
            let n = n.min(5);
            state.set_stars(&path, if n == stars { 0 } else { n });
        }
    }
    if selected {
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(1.5, visuals.selection.stroke.color),
            StrokeKind::Inside,
        );
    } else if response.hovered() {
        ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_gray(110)), StrokeKind::Inside);
    }
    response.context_menu(|ui| {
        ui.label(egui::RichText::new("Stars").weak());
        for n in (0..=5u8).rev() {
            let label = if n == 0 { "No stars".to_string() } else { "★".repeat(n as usize) };
            if ui.selectable_label(n == stars, label).clicked() {
                state.set_stars(&path, n);
                ui.close();
            }
        }
    });
    if response.double_clicked() {
        Click::Double
    } else if response.clicked() {
        Click::Single
    } else {
        Click::None
    }
}

/// "Photos in this folder range from shutter count … to …", per camera
/// model, from the counts the cameras wrote into the photos.
fn shutter_range(ui: &mut Ui, state: &AppState) {
    let mut ranges: std::collections::BTreeMap<&str, (u32, u32)> = std::collections::BTreeMap::new();
    for facts in state.facts.values() {
        if let Some(count) = facts.shutter_count {
            let range = ranges.entry(facts.model.as_str()).or_insert((count, count));
            *range = (range.0.min(count), range.1.max(count));
        }
    }
    for (model, (low, high)) in &ranges {
        let who = if ranges.len() > 1 { format!(" ({model})") } else { String::new() };
        let text = if low == high {
            format!("Shutter count of the photos in this folder{who}: {low}")
        } else {
            format!("Photos in this folder{who} range from shutter count {low} to {high}")
        };
        ui.label(egui::RichText::new(text).weak()).on_hover_text(
            "The count the camera wrote into each photo when it was taken: mechanical shutter actuations. \
             Electronic-shutter shots may not be counted, and a firmware update can reset it.",
        );
    }
}

/// The row of filters and the sort order.
fn filter_bar(ui: &mut Ui, state: &mut AppState) {
    use crate::photo::SortBy;
    // The values that occur in this folder, for the pick-lists.
    let mut lenses: Vec<String> = state.facts.values().map(|f| f.lens.clone()).filter(|l| !l.is_empty()).collect();
    lenses.sort();
    lenses.dedup();
    let mut apertures: Vec<(f64, String)> =
        state.facts.values().filter_map(|f| f.f_number.map(|n| (n, f.aperture_label()))).collect();
    apertures.sort_by(|a, b| a.0.total_cmp(&b.0));
    apertures.dedup_by(|a, b| a.1 == b.1);
    let mut shutters: Vec<(f64, String)> =
        state.facts.values().filter_map(|f| f.shutter.map(|s| (s, f.shutter_label.clone()))).collect();
    shutters.sort_by(|a, b| a.0.total_cmp(&b.0));
    shutters.dedup_by(|a, b| a.1 == b.1);
    let mut isos: Vec<u32> = state.facts.values().filter_map(|f| f.iso).collect();
    isos.sort();
    isos.dedup();

    let filter = &mut state.filter;
    // Wrap within what can be seen of the panel.
    let visible = (ui.clip_rect().right().min(ui.max_rect().right()) - ui.cursor().left()).max(120.0);
    ui.allocate_ui(Vec2::new(visible, 0.0), |ui| {
    ui.set_max_width(visible - 8.0);
    ui.horizontal_wrapped(|ui| {
        ui.label("Show");
        // Exactly that many stars, not "that many and up".
        let star_choices: Vec<(u8, String)> =
            (1..=5u8).rev().map(|n| (n, "★".repeat(n as usize))).chain([(0, "No stars".to_string())]).collect();
        pick(ui, "filter_stars", "Any stars", &mut filter.stars, star_choices, 96.0);
        pick(ui, "filter_lens", "Any lens", &mut filter.lens, lenses.into_iter().map(|l| (l.clone(), l)).collect(), 150.0);
        pick(ui, "filter_aperture", "Any aperture", &mut filter.aperture, apertures.into_iter().map(|(_, l)| (l.clone(), l)).collect(), 96.0);
        pick(ui, "filter_shutter", "Any shutter", &mut filter.shutter, shutters.into_iter().map(|(_, l)| (l.clone(), l)).collect(), 96.0);
        pick(ui, "filter_iso", "Any ISO", &mut filter.iso, isos.into_iter().map(|i| (i, format!("ISO {i}"))).collect(), 84.0);
        if ui.add_enabled(filter.is_filtering(), egui::Button::new("Clear")).on_hover_text("Show every photo again.").clicked() {
            filter.clear();
        }
        if ui.available_size_before_wrap().x < 250.0 {
            ui.end_row();
        } else {
            ui.separator();
        }
        ui.label("Sort");
        egui::ComboBox::from_id_salt("sort_by").selected_text(filter.sort.label()).width(104.0).show_ui(ui, |ui| {
            for sort in SortBy::ALL {
                ui.selectable_value(&mut filter.sort, sort, sort.label());
            }
        });
        let direction = if filter.descending { "Largest first" } else { "Smallest first" };
        if ui.button(direction).on_hover_text("Click to turn the order round.").clicked() {
            filter.descending = !filter.descending;
        }
    });
    });
}

/// A pick-list for one filter: "any", or one of the values in the folder.
fn pick<T: Clone + PartialEq>(ui: &mut Ui, id: &str, any: &str, value: &mut Option<T>, choices: Vec<(T, String)>, width: f32) {
    let shown = match value {
        Some(v) => choices.iter().find(|(c, _)| c == v).map(|(_, l)| l.clone()).unwrap_or_else(|| any.to_string()),
        None => any.to_string(),
    };
    // A pick-list does not wrap by itself: start a new row if it will not fit.
    if ui.available_size_before_wrap().x < width + 30.0 {
        ui.end_row();
    }
    egui::ComboBox::from_id_salt(id).selected_text(shown).width(width).show_ui(ui, |ui| {
        ui.selectable_value(value, None, any);
        for (choice, label) in choices {
            ui.selectable_value(value, Some(choice), label);
        }
    });
}
