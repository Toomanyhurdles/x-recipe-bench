//! One file per panel. Each has a single `show(ui, state)` function.

pub mod camera;
pub mod crop;
pub mod editor;
pub mod info;
pub mod library;
pub mod log;
pub mod pad;
pub mod preview;
pub mod recipes;
pub mod slots;

use crate::border::Border;
use crate::photo::Crop;
use crate::orientation::Orientation;
use eframe::egui::{self, Rect, Ui, Vec2};

/// Draws `texture` as large as fits in `rect`, centred, keeping its shape,
/// turned as `orientation` says.
pub fn paint_fitted(ui: &Ui, rect: Rect, texture: &egui::TextureHandle, orientation: Orientation, crop: Option<Crop>) {
    paint_fitted_with_border(ui, rect, texture, orientation, crop, Border::default(), View::default());
}

/// How far the picture is zoomed in and where it is looking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// 1 shows the whole picture; 2 shows it twice as large.
    pub zoom: f32,
    /// How far the picture is shifted, as a fraction of the area it is
    /// drawn in (before zooming).
    pub pan: Vec2,
}

impl Default for View {
    fn default() -> Self {
        View { zoom: 1.0, pan: Vec2::ZERO }
    }
}

/// As [`paint_fitted`], with the border around the picture and zoomed and
/// shifted as `view` says. Picture and border together fill `rect` as far
/// as they can at zoom 1. Returns how many screen points one pixel of the
/// picture takes at zoom 1.
pub fn paint_fitted_with_border(
    ui: &Ui,
    rect: Rect,
    texture: &egui::TextureHandle,
    orientation: Orientation,
    crop: Option<Crop>,
    border: Border,
    view: View,
) -> Option<f32> {
    let whole = orientation.shown_size(texture.size_vec2());
    if whole.x <= 0.0 || whole.y <= 0.0 {
        return None;
    }
    // The part of the upright photo to show: all of it, or the crop.
    let crop = crop.filter(|c| !c.is_none()).map(|c| c.fitted(whole.x / whole.y));
    let (size, part) = match crop {
        Some(c) => (Vec2::new(c.w * whole.x, c.h * whole.y), c.corners(whole.x / whole.y)),
        None => (whole, [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]),
    };
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let centre = rect.center() + view.pan * rect.size() * view.zoom;
    let edge = size.x.min(size.y) * border.fraction();
    let extra = size.x.min(size.y) * border.bottom_fraction();
    let outer = size + Vec2::new(2.0 * edge, 2.0 * edge + extra);
    let fit = (rect.width() / outer.x).min(rect.height() / outer.y);
    let scale = fit * view.zoom;
    // The picture sits above the middle when the bottom edge is wider.
    let fitted = Rect::from_center_size(centre - Vec2::new(0.0, extra * scale / 2.0), size * scale);
    if edge > 0.0 {
        let framed = Rect::from_center_size(centre, outer * scale);
        let [r, g, b] = border.color;
        painter.rect_filled(framed, 0.0, egui::Color32::from_rgb(r, g, b));
        // A hairline, so the border's edge shows on a light background.
        // It is not part of the exported picture.
        let hairline = ui.visuals().widgets.noninteractive.bg_stroke;
        painter.rect_stroke(framed, 0.0, hairline, egui::StrokeKind::Outside);
    }

    // Four corners, clockwise from the top left, each showing the point of
    // the stored picture that belongs there.
    let positions = [fitted.left_top(), fitted.right_top(), fitted.right_bottom(), fitted.left_bottom()];
    let mut mesh = egui::Mesh::with_texture(texture.id());
    // Where a point of the upright photo is in the stored picture.
    let [top_left, top_right, _, bottom_left] = orientation.corners();
    let stored = |(u, v): (f32, f32)| top_left + (top_right - top_left) * u + (bottom_left - top_left) * v;
    for (position, point) in positions.into_iter().zip(part) {
        mesh.vertices.push(egui::epaint::Vertex { pos: position, uv: stored(point), color: egui::Color32::WHITE });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
    Some(fit)
}

/// A muted line of explanatory text.
pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).weak());
}

pub const GAP: Vec2 = Vec2::new(8.0, 8.0);
