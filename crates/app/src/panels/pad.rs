//! A square colour pad for choosing two linked values with one drag, as the
//! camera's own menu does for white balance shift and monochromatic colour.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// The colours at the pad's corners.
pub struct Corners {
    pub top_left: Color32,
    pub top_right: Color32,
    pub bottom_left: Color32,
    pub bottom_right: Color32,
}

/// White balance shift: right adds red, up adds blue.
pub const WB_SHIFT: Corners = Corners {
    top_left: Color32::from_rgb(40, 110, 215),
    top_right: Color32::from_rgb(190, 60, 190),
    bottom_left: Color32::from_rgb(70, 190, 90),
    bottom_right: Color32::from_rgb(235, 150, 40),
};

/// Monochromatic colour: right is green, left magenta; up is warm, down cool.
pub const MONO_COLOUR: Corners = Corners {
    top_left: Color32::from_rgb(215, 90, 120),
    top_right: Color32::from_rgb(150, 190, 70),
    bottom_left: Color32::from_rgb(130, 80, 200),
    bottom_right: Color32::from_rgb(50, 170, 150),
};

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let channel = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(channel(a.r(), b.r()), channel(a.g(), b.g()), channel(a.b(), b.b()))
}

/// Draws the pad. `x` grows to the right and `y` grows upwards; both run
/// from `-range` to `range` in whole steps. Returns true if a value changed.
pub fn show(ui: &mut Ui, x: &mut i32, y: &mut i32, range: i32, corners: &Corners, side: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), Sense::click_and_drag());
    let mut changed = false;

    if let Some(pointer) = response.interact_pointer_pos().filter(|_| response.clicked() || response.dragged()) {
        let fx = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let fy = ((pointer.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        let new_x = ((fx * 2.0 - 1.0) * range as f32).round() as i32;
        let new_y = ((1.0 - fy * 2.0) * range as f32).round() as i32;
        if (new_x, new_y) != (*x, *y) {
            *x = new_x;
            *y = new_y;
            changed = true;
        }
    }
    // A double click puts the dot back in the middle.
    if response.double_clicked() && (*x, *y) != (0, 0) {
        *x = 0;
        *y = 0;
        changed = true;
    }

    if ui.is_rect_visible(rect) {
        // The colour field: a grid of vertices blended between the corners.
        const STEPS: usize = 8;
        let mut mesh = egui::Mesh::default();
        for row in 0..=STEPS {
            for column in 0..=STEPS {
                let (tx, ty) = (column as f32 / STEPS as f32, row as f32 / STEPS as f32);
                let top = mix(corners.top_left, corners.top_right, tx);
                let bottom = mix(corners.bottom_left, corners.bottom_right, tx);
                let position = Pos2::new(rect.left() + tx * rect.width(), rect.top() + ty * rect.height());
                mesh.colored_vertex(position, mix(top, bottom, ty));
            }
        }
        let stride = (STEPS + 1) as u32;
        for row in 0..STEPS as u32 {
            for column in 0..STEPS as u32 {
                let i = row * stride + column;
                mesh.add_triangle(i, i + 1, i + stride);
                mesh.add_triangle(i + 1, i + stride + 1, i + stride);
            }
        }
        let painter = ui.painter_at(rect);
        painter.add(egui::Shape::mesh(mesh));

        let line = Stroke::new(1.0, Color32::from_white_alpha(110));
        painter.line_segment([rect.center_top(), rect.center_bottom()], line);
        painter.line_segment([rect.left_center(), rect.right_center()], line);

        let dot = Pos2::new(
            rect.center().x + (*x as f32 / range as f32) * rect.width() / 2.0,
            rect.center().y - (*y as f32 / range as f32) * rect.height() / 2.0,
        );
        let dot = Pos2::new(dot.x.clamp(rect.left() + 4.0, rect.right() - 4.0), dot.y.clamp(rect.top() + 4.0, rect.bottom() - 4.0));
        ui.painter().circle(dot, 5.0, Color32::BLACK, Stroke::new(2.0, Color32::WHITE));
        ui.painter().rect_stroke(
            Rect::from_min_max(rect.min, rect.max),
            2.0,
            Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
    }
    changed
}
