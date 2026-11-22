//! The crop editor shown in the Preview while a crop is being adjusted:
//! the whole photo with a frame to drag, a choice of shapes, and a tilt
//! to level the horizon. The arithmetic is in `photo.rs` (`Crop`).

use super::hint;
use crate::photo::{Crop, MAX_TILT};
use crate::state::{AppState, Slot};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::path::PathBuf;

/// The shapes offered. `None` is the photo's own shape.
const SHAPES: [(&str, Option<(f32, f32)>); 6] = [
    ("Original", None),
    ("3:2", Some((3.0, 2.0))),
    ("4:3", Some((4.0, 3.0))),
    ("1:1", Some((1.0, 1.0))),
    ("5:4", Some((5.0, 4.0))),
    ("16:9", Some((16.0, 9.0))),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Free,
    /// Index into [`SHAPES`].
    Fixed(usize),
    Custom,
}

/// What the mouse has hold of.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Grab {
    /// A corner: 0 top left, 1 top right, 2 bottom right, 3 bottom left.
    Corner(usize),
    Frame,
}

/// A crop being adjusted. Nothing is stored until "Done".
pub struct CropEdit {
    pub raf: PathBuf,
    pub crop: Crop,
    pub shape: Shape,
    /// Use the shape on its side (2:3 instead of 3:2).
    pub sideways: bool,
    pub custom: (f32, f32),
    grab: Option<Grab>,
}

impl CropEdit {
    pub fn start(raf: PathBuf, crop: Option<Crop>) -> CropEdit {
        CropEdit { raf, crop: crop.unwrap_or_default(), shape: Shape::Free, sideways: false, custom: (7.0, 5.0), grab: None }
    }

    /// Width ÷ height the frame must keep, or `None` if it is free.
    fn ratio(&self, aspect: f32) -> Option<f32> {
        let ratio = match self.shape {
            Shape::Free => return None,
            Shape::Fixed(index) => SHAPES[index].1.map(|(w, h)| w / h).unwrap_or(aspect),
            Shape::Custom => self.custom.0.max(0.1) / self.custom.1.max(0.1),
        };
        Some(if self.sideways { 1.0 / ratio } else { ratio })
    }
}

/// The editor: its controls, then the photo with the frame.
pub fn show(ui: &mut Ui, state: &mut AppState) {
    let Some(mut edit) = state.crop_edit.take() else { return };
    // The photo to crop: the camera's version if there is one, otherwise
    // the picture stored in the file.
    let texture = state
        .current_key()
        .and_then(|key| state.developed_for(&key).cloned())
        .or_else(|| match &state.embedded {
            Some((path, Slot::Ready(texture))) if path == &edit.raf => Some(texture.clone()),
            _ => None,
        });
    let orientation = state.orientation_of(&edit.raf);
    let Some(texture) = texture else {
        hint(ui, "The photo is still loading.");
        state.crop_edit = Some(edit);
        return;
    };
    let size = orientation.shown_size(texture.size_vec2());
    let aspect = size.x / size.y;

    let mut finished = None;
    ui.horizontal_wrapped(|ui| {
        ui.strong("Crop");
        ui.separator();
        ui.label("Shape");
        let before = (edit.shape, edit.sideways, edit.custom);
        let name = match edit.shape {
            Shape::Free => "Free",
            Shape::Fixed(index) => SHAPES[index].0,
            Shape::Custom => "Custom",
        };
        egui::ComboBox::from_id_salt("crop_shape").selected_text(name).width(90.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut edit.shape, Shape::Free, "Free");
            for (index, (name, _)) in SHAPES.iter().enumerate() {
                ui.selectable_value(&mut edit.shape, Shape::Fixed(index), *name);
            }
            ui.selectable_value(&mut edit.shape, Shape::Custom, "Custom");
        });
        if edit.shape == Shape::Custom {
            ui.add(egui::DragValue::new(&mut edit.custom.0).range(1.0..=50.0).speed(0.1).max_decimals(1));
            ui.label(":");
            ui.add(egui::DragValue::new(&mut edit.custom.1).range(1.0..=50.0).speed(0.1).max_decimals(1));
        }
        if edit.shape != Shape::Free {
            ui.checkbox(&mut edit.sideways, "On its side").on_hover_text("Swap width and height: 2:3 instead of 3:2.");
        }
        if before != (edit.shape, edit.sideways, edit.custom) {
            if let Some(ratio) = edit.ratio(aspect) {
                edit.crop = Crop::largest(ratio, edit.crop.angle, aspect);
            }
        }
        ui.separator();
        ui.label("Straighten");
        let mut angle = edit.crop.angle;
        ui.spacing_mut().slider_width = 130.0;
        let changed = ui.add(egui::Slider::new(&mut angle, -MAX_TILT..=MAX_TILT).step_by(0.1).suffix("°")).changed();
        if changed {
            // The frame shrinks as needed to stay on the tilted photo.
            edit.crop = Crop { angle, ..edit.crop }.fitted(aspect);
        }
        ui.separator();
        if ui.button("Done").clicked() {
            finished = Some(true);
        }
        if ui.button("Reset").on_hover_text("Back to the whole photo, untilted.").clicked() {
            edit.crop = Crop::default();
            edit.shape = Shape::Free;
        }
        if ui.button("Cancel").clicked() {
            finished = Some(false);
        }
    });
    ui.separator();

    let (area, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter().with_clip_rect(area);
    painter.rect_filled(area, 0.0, ui.visuals().extreme_bg_color);

    // The photo, tilted, with room around it.
    let scale = ((area.width() - 40.0) / size.x).min((area.height() - 40.0) / size.y).max(0.01);
    let centre = area.center();
    let (sin, cos) = edit.crop.angle.to_radians().sin_cos();
    let on_screen = |u: f32, v: f32| {
        let (x, y) = ((u - 0.5) * size.x * scale, (v - 0.5) * size.y * scale);
        centre + Vec2::new(x * cos - y * sin, x * sin + y * cos)
    };
    let mut mesh = egui::Mesh::with_texture(texture.id());
    let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
    for ((u, v), uv) in corners.into_iter().zip(orientation.corners()) {
        mesh.vertices.push(egui::epaint::Vertex { pos: on_screen(u, v), uv, color: Color32::WHITE });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));

    // The frame stays level on the screen while the photo tilts under it.
    let unit = size * scale;
    let frame_of = |c: &Crop| {
        Rect::from_min_size(centre + Vec2::new((c.x - 0.5) * unit.x, (c.y - 0.5) * unit.y), Vec2::new(c.w * unit.x, c.h * unit.y))
    };
    let frame = frame_of(&edit.crop);
    let handles = [frame.left_top(), frame.right_top(), frame.right_bottom(), frame.left_bottom()];

    // Dragging: a corner resizes, the inside moves the frame.
    if response.drag_started() {
        edit.grab = response.interact_pointer_pos().and_then(|p: Pos2| {
            handles
                .iter()
                .position(|h| h.distance(p) <= 16.0)
                .map(Grab::Corner)
                .or_else(|| frame.contains(p).then_some(Grab::Frame))
        });
    }
    if response.dragged() {
        let d = response.drag_delta() / unit;
        let c = edit.crop;
        let candidate = match edit.grab {
            Some(Grab::Frame) => Some(Crop { x: c.x + d.x, y: c.y + d.y, ..c }),
            Some(Grab::Corner(corner)) => {
                let (left, top) = (corner == 0 || corner == 3, corner == 0 || corner == 1);
                // The opposite corner stays where it is.
                let (anchor_x, anchor_y) = (if left { c.x + c.w } else { c.x }, if top { c.y + c.h } else { c.y });
                let mut w = if left { c.w - d.x } else { c.w + d.x };
                let mut h = if top { c.h - d.y } else { c.h + d.y };
                if let Some(ratio) = edit.ratio(aspect) {
                    // Follow whichever direction the mouse moved more.
                    if d.x.abs() * unit.x >= d.y.abs() * unit.y {
                        h = w * aspect / ratio;
                    } else {
                        w = h * ratio / aspect;
                    }
                }
                Some(Crop {
                    x: if left { anchor_x - w } else { anchor_x },
                    y: if top { anchor_y - h } else { anchor_y },
                    w,
                    h,
                    ..c
                })
            }
            None => None,
        };
        // A frame that would leave the photo, or get too small, is not taken.
        if let Some(candidate) = candidate {
            if candidate.w >= 0.05 && candidate.h >= 0.05 && candidate.fits(aspect) {
                edit.crop = candidate;
            }
        }
    }
    if response.drag_stopped() {
        edit.grab = None;
    }

    // Darken what will be cut off, then draw the frame and its handles.
    let frame = frame_of(&edit.crop);
    let shade = Color32::from_black_alpha(150);
    painter.rect_filled(Rect::from_min_max(area.min, egui::pos2(area.right(), frame.top())), 0.0, shade);
    painter.rect_filled(Rect::from_min_max(egui::pos2(area.left(), frame.bottom()), area.max), 0.0, shade);
    painter.rect_filled(Rect::from_min_max(egui::pos2(area.left(), frame.top()), frame.left_bottom()), 0.0, shade);
    painter.rect_filled(Rect::from_min_max(frame.right_top(), egui::pos2(area.right(), frame.bottom())), 0.0, shade);
    let line = Stroke::new(1.0, Color32::from_white_alpha(90));
    for third in [1.0 / 3.0, 2.0 / 3.0] {
        let (x, y) = (frame.left() + frame.width() * third, frame.top() + frame.height() * third);
        painter.line_segment([egui::pos2(x, frame.top()), egui::pos2(x, frame.bottom())], line);
        painter.line_segment([egui::pos2(frame.left(), y), egui::pos2(frame.right(), y)], line);
    }
    painter.rect_stroke(frame, 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Middle);
    for handle in [frame.left_top(), frame.right_top(), frame.right_bottom(), frame.left_bottom()] {
        painter.rect_filled(Rect::from_center_size(handle, Vec2::splat(11.0)), 2.0, Color32::WHITE);
        painter.rect_stroke(Rect::from_center_size(handle, Vec2::splat(11.0)), 2.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Middle);
    }
    // The shape of the result, for reference.
    let ratio = edit.crop.ratio(aspect);
    painter.text(
        frame.center_bottom() + Vec2::new(0.0, 6.0),
        egui::Align2::CENTER_TOP,
        format!("{:.0}% × {:.0}% of the photo  ·  shape {:.2}:1", edit.crop.w * 100.0, edit.crop.h * 100.0, ratio),
        egui::FontId::proportional(12.0),
        Color32::from_gray(220),
    );

    match finished {
        Some(true) => {
            let raf = edit.raf.clone();
            state.set_crop(&raf, Some(edit.crop));
        }
        Some(false) => {}
        None => state.crop_edit = Some(edit),
    }
}

/// A row of five stars to click. Clicking the current number of stars
/// again takes them away. Returns the new number if it changed.
pub fn stars_row(ui: &mut Ui, stars: u8) -> Option<u8> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for n in 1..=5u8 {
            let sign = if n <= stars { "★" } else { "☆" };
            let text = egui::RichText::new(sign).size(15.0);
            let hint = if n == stars { "Click to remove the stars".to_string() } else { format!("{n} of 5 (key {n})") };
            if ui.add(egui::Label::new(text).sense(Sense::click())).on_hover_text(hint).clicked() {
                picked = Some(if n == stars { 0 } else { n });
            }
        }
    });
    picked
}
