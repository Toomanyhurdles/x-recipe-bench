//! The parts of a window the system normally draws, drawn by the app
//! instead: the title bar with the logo and name, the minimise / maximise /
//! close buttons, the edges that resize, and the outline.
//!
//! Whether the app draws these at all is decided by craft-window (see
//! `title_bar` in main.rs): by default it does, so the title can carry the
//! logo and follow the chosen appearance; with "Use the system title bar"
//! the desktop draws them. craft-window also says which end of the bar the
//! buttons go at on this desktop, and makes the bar's empty space move the
//! window the way the system's own title bar does.

use eframe::egui::{self, Color32, CursorIcon, Rect, Sense, Stroke, Ui, Vec2};

/// A title-bar button the user pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    Minimise,
    Maximise,
    Close,
}

/// The logo and the app's name, styled as one mark: the X in the accent
/// colour, like the green X of the logo.
pub fn brand(ui: &mut Ui, logo: &egui::TextureHandle) {
    let size = egui::TextStyle::Body.resolve(ui.style()).size;
    // Its own group, so the tight spacing between the logo and the two
    // parts of the name does not spread to the rest of the bar.
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add_space(2.0);
        ui.add(egui::Image::new(logo).fit_to_exact_size(Vec2::splat(size * 1.5)));
        let accent =
            if ui.visuals().dark_mode { Color32::from_rgb(88, 200, 120) } else { Color32::from_rgb(30, 140, 70) };
        let strong = ui.visuals().strong_text_color();
        ui.label(egui::RichText::new("X").size(size * 1.25).color(accent));
        ui.label(egui::RichText::new("Recipe Bench").size(size * 1.25).color(strong));
    });
}

/// How wide [`brand`] draws, so it can be centred.
pub fn brand_width(ui: &Ui) -> f32 {
    let size = egui::TextStyle::Body.resolve(ui.style()).size;
    let font = egui::FontId::proportional(size * 1.25);
    let text = |t: &str| ui.painter().layout_no_wrap(t.to_string(), font.clone(), Color32::WHITE).size().x;
    2.0 + size * 1.5 + text("X") + text("Recipe Bench") + 4.0 * 3.0
}

/// The caption buttons to draw, or `None` when the desktop draws the title bar.
pub fn captions(frame: &craft_window::policy::Frame) -> Option<&craft_window::policy::Captions> {
    match frame {
        craft_window::policy::Frame::Integrated(captions) => Some(captions),
        _ => None,
    }
}

/// Makes the bar's empty space behave like a title bar (craft-window): drag
/// it to move the window; a double-click does what the system's setting says
/// (usually maximise). Call this before adding the bar's own controls, so
/// they stay clickable on top of it.
pub fn drag_area(ui: &mut Ui) {
    let rect = ui.max_rect();
    craft_window::title_bar::empty_area(ui, rect);
}

/// Window buttons, in the order the desktop lists them for one end of the
/// bar (outermost last). Call inside a right-to-left layout for the right
/// end, a left-to-right one for the left end: either way the outermost
/// button is drawn first.
pub fn window_buttons(ui: &mut Ui, buttons: &[craft_window::Button], maximised: bool) -> Option<Pressed> {
    let mut pressed = None;
    ui.spacing_mut().item_spacing.x = 0.0;
    for b in buttons.iter().rev() {
        let (kind, hint) = match b {
            craft_window::Button::Close => (Pressed::Close, "Close"),
            craft_window::Button::Maximize if maximised => (Pressed::Maximise, "Restore the previous size"),
            craft_window::Button::Maximize => (Pressed::Maximise, "Maximise (leaves room for separate panel windows)"),
            craft_window::Button::Minimize => (Pressed::Minimise, "Minimise the app and its panel windows"),
        };
        if button(ui, kind, maximised, hint) {
            pressed = Some(kind);
        }
    }
    pressed
}

/// One title-bar button, with its sign drawn as lines so it does not
/// depend on a font having the right symbols.
fn button(ui: &mut Ui, kind: Pressed, maximised: bool, hint: &str) -> bool {
    let height = ui.spacing().interact_size.y.max(20.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(height * 1.5, height), Sense::click());
    let hovered = response.hovered();
    if hovered {
        let fill = if kind == Pressed::Close { Color32::from_rgb(200, 60, 50) } else { ui.visuals().widgets.hovered.bg_fill };
        ui.painter().rect_filled(rect, 3.0, fill);
    }
    let colour = if hovered && kind == Pressed::Close { Color32::WHITE } else { ui.visuals().text_color() };
    let stroke = Stroke::new(1.3, colour);
    let sign = Rect::from_center_size(rect.center(), Vec2::splat(9.0));
    let painter = ui.painter();
    match kind {
        Pressed::Close => {
            painter.line_segment([sign.left_top(), sign.right_bottom()], stroke);
            painter.line_segment([sign.right_top(), sign.left_bottom()], stroke);
        }
        Pressed::Minimise => {
            painter.line_segment([sign.left_bottom(), sign.right_bottom()], stroke);
        }
        Pressed::Maximise if maximised => {
            // Two overlapping squares: "restore".
            let front = Rect::from_min_size(sign.min + Vec2::new(0.0, 2.5), Vec2::splat(6.5));
            let back = Rect::from_min_size(sign.min + Vec2::new(2.5, 0.0), Vec2::splat(6.5));
            painter.rect_stroke(back, 0.0, stroke, egui::StrokeKind::Middle);
            painter.rect_filled(front, 0.0, ui.visuals().panel_fill);
            painter.rect_stroke(front, 0.0, stroke, egui::StrokeKind::Middle);
        }
        Pressed::Maximise => {
            painter.rect_stroke(sign, 0.0, stroke, egui::StrokeKind::Middle);
        }
    }
    response.on_hover_text(hint).clicked()
}

/// How close to a window's edge the mouse has to be to resize it.
const EDGE: f32 = 5.0;

/// Lets the window be resized by its edges and corners, which the system
/// no longer does for a window without its frame. Call once per window,
/// inside that window, after its contents.
pub fn resize_edges(ctx: &egui::Context) {
    use egui::viewport::ResizeDirection as D;
    // A maximised window has no edges to drag.
    if ctx.input(|i| i.viewport().maximized).unwrap_or(false) {
        return;
    }
    let area = ctx.content_rect();
    let (position, pressed) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.primary_pressed()));
    let Some(p) = position else { return };
    if !area.contains(p) {
        return;
    }
    let (left, right) = (p.x - area.left() <= EDGE, area.right() - p.x <= EDGE);
    let (top, bottom) = (p.y - area.top() <= EDGE, area.bottom() - p.y <= EDGE);
    let (direction, cursor) = match (left, right, top, bottom) {
        (true, _, true, _) => (D::NorthWest, CursorIcon::ResizeNwSe),
        (_, true, _, true) => (D::SouthEast, CursorIcon::ResizeNwSe),
        (_, true, true, _) => (D::NorthEast, CursorIcon::ResizeNeSw),
        (true, _, _, true) => (D::SouthWest, CursorIcon::ResizeNeSw),
        (true, ..) => (D::West, CursorIcon::ResizeHorizontal),
        (_, true, ..) => (D::East, CursorIcon::ResizeHorizontal),
        (_, _, true, _) => (D::North, CursorIcon::ResizeVertical),
        (_, _, _, true) => (D::South, CursorIcon::ResizeVertical),
        _ => return,
    };
    ctx.set_cursor_icon(cursor);
    if pressed {
        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
    }
}

/// A thin line around the window, so it has a visible edge without the
/// system's frame.
pub fn outline(ctx: &egui::Context) {
    let colour = ctx.global_style().visuals.widgets.noninteractive.bg_stroke.color;
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("window_outline")));
    painter.rect_stroke(ctx.content_rect(), 0.0, Stroke::new(1.0, colour), egui::StrokeKind::Inside);
}
