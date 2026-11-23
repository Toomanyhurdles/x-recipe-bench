//! How the app looks: dark or light, in one of five appearances.
//!
//! An appearance is a [`Recipe`] of colours, roundness, spacing and text
//! size; [`apply`] turns one into the interface library's style. To add an
//! appearance: add a variant to [`Appearance`], list it in `ALL`, and give
//! it a recipe in [`recipe`].

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    UltraModern,
    Traditional,
    CompactPro,
    SoftRounded,
    Darkroom,
}

impl Appearance {
    pub const ALL: [Appearance; 5] = [
        Appearance::UltraModern,
        Appearance::Traditional,
        Appearance::CompactPro,
        Appearance::SoftRounded,
        Appearance::Darkroom,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Appearance::UltraModern => "Ultra modern",
            Appearance::Traditional => "Traditional",
            Appearance::CompactPro => "Compact Pro",
            Appearance::SoftRounded => "Soft Rounded",
            Appearance::Darkroom => "Darkroom",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Appearance::UltraModern => "Flat and borderless, with a bright accent colour.",
            Appearance::Traditional => "Classic desktop look: outlined buttons and square corners.",
            Appearance::CompactPro => "Small text and tight spacing, to fit more on screen.",
            Appearance::SoftRounded => "Large rounded controls with generous spacing.",
            Appearance::Darkroom => "Neutral greys with an amber accent, so nothing tints your view of the photo.",
        }
    }
}

/// The ingredients of an appearance, for one of dark or light.
struct Recipe {
    /// Background of panels.
    panel: Color32,
    /// Background of windows and menus.
    window: Color32,
    /// Background behind photos and inside text boxes.
    well: Color32,
    /// Resting background of buttons.
    button: Color32,
    button_hovered: Color32,
    button_pressed: Color32,
    /// Outline of buttons; `None` for a flat look.
    outline: Option<Color32>,
    /// Lines between panels and around windows.
    divider: Color32,
    text: Color32,
    strong_text: Color32,
    /// Highlight colour: selections, focus, links.
    accent: Color32,
    /// Text drawn on top of the accent colour.
    on_accent: Color32,
    warning: Color32,
    /// Corner roundness of controls, and of windows and menus.
    radius: u8,
    window_radius: u8,
    /// Space between controls, and inside buttons.
    item_spacing: Vec2,
    button_padding: Vec2,
    /// Height of a row of controls.
    row_height: f32,
    text_size: f32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// A colour part of the way from `a` to `b`.
fn mix(a: Color32, b: Color32, amount: f32) -> Color32 {
    let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).round() as u8;
    Color32::from_rgb(channel(a.r(), b.r()), channel(a.g(), b.g()), channel(a.b(), b.b()))
}

fn recipe(appearance: Appearance, dark: bool) -> Recipe {
    // Sizes shared by most appearances.
    let regular = (Vec2::new(8.0, 5.0), Vec2::new(6.0, 2.0), 20.0, 13.5);
    match (appearance, dark) {
        (Appearance::UltraModern, true) => Recipe {
            panel: rgb(0x121317),
            window: rgb(0x191B21),
            well: rgb(0x0A0B0D),
            button: rgb(0x22252D),
            button_hovered: rgb(0x2E323D),
            button_pressed: rgb(0x3A3F4D),
            outline: None,
            divider: rgb(0x23262E),
            text: rgb(0xC9CEDA),
            strong_text: rgb(0xFFFFFF),
            accent: rgb(0x4463E0),
            on_accent: rgb(0xFFFFFF),
            warning: rgb(0xFFB454),
            radius: 7,
            window_radius: 12,
            item_spacing: regular.0,
            button_padding: Vec2::new(8.0, 3.0),
            row_height: 22.0,
            text_size: regular.3,
        },
        (Appearance::UltraModern, false) => Recipe {
            panel: rgb(0xF6F7F9),
            window: rgb(0xFFFFFF),
            well: rgb(0xE9EBEF),
            button: rgb(0xE6E8EE),
            button_hovered: rgb(0xD9DCE5),
            button_pressed: rgb(0xC9CDD9),
            outline: None,
            divider: rgb(0xDFE2E8),
            text: rgb(0x343946),
            strong_text: rgb(0x0E1016),
            accent: rgb(0x3F5FE0),
            on_accent: rgb(0xFFFFFF),
            warning: rgb(0xA85A00),
            radius: 7,
            window_radius: 12,
            item_spacing: regular.0,
            button_padding: Vec2::new(8.0, 3.0),
            row_height: 22.0,
            text_size: regular.3,
        },
        (Appearance::Traditional, true) => Recipe {
            panel: rgb(0x3C3F41),
            window: rgb(0x45484A),
            well: rgb(0x2B2B2B),
            button: rgb(0x4C5052),
            button_hovered: rgb(0x595D60),
            button_pressed: rgb(0x666B6E),
            outline: Some(rgb(0x6B6F72)),
            divider: rgb(0x2A2C2D),
            text: rgb(0xD4D4D4),
            strong_text: rgb(0xFFFFFF),
            accent: rgb(0x3D74C4),
            on_accent: rgb(0xFFFFFF),
            warning: rgb(0xF0B35A),
            radius: 2,
            window_radius: 3,
            item_spacing: regular.0,
            button_padding: regular.1,
            row_height: regular.2,
            text_size: regular.3,
        },
        (Appearance::Traditional, false) => Recipe {
            panel: rgb(0xECECEC),
            window: rgb(0xF5F5F5),
            well: rgb(0xFFFFFF),
            button: rgb(0xE1E1E1),
            button_hovered: rgb(0xD4DEEC),
            button_pressed: rgb(0xC2D2E8),
            outline: Some(rgb(0xA9A9A9)),
            divider: rgb(0xBDBDBD),
            text: rgb(0x2A2A2A),
            strong_text: rgb(0x000000),
            accent: rgb(0x2D62B3),
            on_accent: rgb(0xFFFFFF),
            warning: rgb(0x9A5200),
            radius: 2,
            window_radius: 3,
            item_spacing: regular.0,
            button_padding: regular.1,
            row_height: regular.2,
            text_size: regular.3,
        },
        (Appearance::CompactPro, true) => Recipe {
            panel: rgb(0x2A2A2C),
            window: rgb(0x323234),
            well: rgb(0x1C1C1D),
            button: rgb(0x3A3A3D),
            button_hovered: rgb(0x48484C),
            button_pressed: rgb(0x55555A),
            outline: None,
            divider: rgb(0x1A1A1B),
            text: rgb(0xC4C4C6),
            strong_text: rgb(0xF2F2F3),
            accent: rgb(0x3F73AB),
            on_accent: rgb(0xFFFFFF),
            warning: rgb(0xE8A94F),
            radius: 2,
            window_radius: 4,
            item_spacing: Vec2::new(5.0, 2.0),
            button_padding: Vec2::new(4.0, 1.0),
            row_height: 17.0,
            text_size: 12.0,
        },
        (Appearance::CompactPro, false) => Recipe {
            panel: rgb(0xDCDCDE),
            window: rgb(0xE8E8EA),
            well: rgb(0xF4F4F5),
            button: rgb(0xCDCDD0),
            button_hovered: rgb(0xBFC3CA),
            button_pressed: rgb(0xAEB5C0),
            outline: None,
            divider: rgb(0xB4B4B8),
            text: rgb(0x2C2C2E),
            strong_text: rgb(0x000000),
            accent: rgb(0x2F6297),
            on_accent: rgb(0xFFFFFF),
            warning: rgb(0x8F4E00),
            radius: 2,
            window_radius: 4,
            item_spacing: Vec2::new(5.0, 2.0),
            button_padding: Vec2::new(4.0, 1.0),
            row_height: 17.0,
            text_size: 12.0,
        },
        (Appearance::SoftRounded, true) => Recipe {
            panel: rgb(0x1E2228),
            window: rgb(0x262B33),
            well: rgb(0x15181D),
            button: rgb(0x2F353F),
            button_hovered: rgb(0x3A424E),
            button_pressed: rgb(0x46505E),
            outline: None,
            divider: rgb(0x2B313A),
            text: rgb(0xCDD5DF),
            strong_text: rgb(0xFFFFFF),
            accent: rgb(0x1F8062),
            on_accent: rgb(0xFFFFFF),
            warning: rgb(0xF2B45E),
            radius: 12,
            window_radius: 16,
            item_spacing: Vec2::new(10.0, 7.0),
            button_padding: Vec2::new(11.0, 5.0),
            row_height: 26.0,
            text_size: 14.5,
        },
        (Appearance::SoftRounded, false) => Recipe {
            panel: rgb(0xF3F6F5),
            window: rgb(0xFFFFFF),
            well: rgb(0xE5EBE9),
            button: rgb(0xE0E8E5),
            button_hovered: rgb(0xD0DDD8),
            button_pressed: rgb(0xBDCFC8),
            outline: None,
            divider: rgb(0xD8E0DD),
            text: rgb(0x33403C),
            strong_text: rgb(0x0F1A17),
            accent: rgb(0x1F7F60),
            on_accent: rgb(0xFFFFFF),
            warning: rgb(0x9C5600),
            radius: 12,
            window_radius: 16,
            item_spacing: Vec2::new(10.0, 7.0),
            button_padding: Vec2::new(11.0, 5.0),
            row_height: 26.0,
            text_size: 14.5,
        },
        // Greys only, with equal red, green and blue, so the surroundings
        // do not shift how colours in the photo are seen.
        (Appearance::Darkroom, true) => Recipe {
            panel: rgb(0x1E1E1E),
            window: rgb(0x262626),
            well: rgb(0x121212),
            button: rgb(0x303030),
            button_hovered: rgb(0x3C3C3C),
            button_pressed: rgb(0x484848),
            outline: None,
            divider: rgb(0x141414),
            text: rgb(0xBEBEBE),
            strong_text: rgb(0xEDEDED),
            accent: rgb(0x9A6A1E),
            on_accent: rgb(0xFFFFFF),
            warning: rgb(0xE0A040),
            radius: 4,
            window_radius: 6,
            item_spacing: regular.0,
            button_padding: regular.1,
            row_height: regular.2,
            text_size: regular.3,
        },
        (Appearance::Darkroom, false) => Recipe {
            panel: rgb(0xC8C8C8),
            window: rgb(0xD6D6D6),
            well: rgb(0xB4B4B4),
            button: rgb(0xB9B9B9),
            button_hovered: rgb(0xACACAC),
            button_pressed: rgb(0x9E9E9E),
            outline: None,
            divider: rgb(0xA6A6A6),
            text: rgb(0x242424),
            strong_text: rgb(0x000000),
            accent: rgb(0x7A5210),
            on_accent: rgb(0xFFFFFF),
            warning: rgb(0x6E3B00),
            radius: 4,
            window_radius: 6,
            item_spacing: regular.0,
            button_padding: regular.1,
            row_height: regular.2,
            text_size: regular.3,
        },
    }
}

/// The interface library's style for an appearance.
pub fn style(appearance: Appearance, dark: bool) -> egui::Style {
    let r = recipe(appearance, dark);
    let mut style = egui::Style::default();
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };

    v.panel_fill = r.panel;
    v.window_fill = r.window;
    v.extreme_bg_color = r.well;
    v.faint_bg_color = r.button;
    v.code_bg_color = r.well;
    v.hyperlink_color = r.accent;
    v.warn_fg_color = r.warning;
    v.window_stroke = Stroke::new(1.0, r.divider);
    v.window_corner_radius = CornerRadius::same(r.window_radius);
    v.menu_corner_radius = CornerRadius::same(r.window_radius.min(8));
    v.selection.bg_fill = r.accent;
    v.selection.stroke = Stroke::new(1.0, r.on_accent);
    v.slider_trailing_fill = true;

    let radius = CornerRadius::same(r.radius);
    // Flat appearances still get a faint edge, so an empty tick box or
    // text box can be seen against the panel it sits on.
    let faint_edge = mix(r.button, r.text, 0.22);
    let outline = |colour: Color32| match r.outline {
        Some(_) => Stroke::new(1.0, colour),
        None => Stroke::new(1.0, faint_edge),
    };
    // Fills, corners and text for each state (craft-ui's WidgetPalette), then
    // the per-state edges and text weights this app uses on top.
    craft_ui::theme::WidgetPalette {
        noninteractive: r.panel,
        inactive: r.button,
        hovered: r.button_hovered,
        active: r.button_pressed,
        open: r.button_pressed,
    }
    .apply_to(&mut v.widgets, radius, Stroke::new(1.0, r.text));
    // Things that are not controls: labels, separators, frames.
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, r.divider);
    // Controls at rest.
    v.widgets.inactive.bg_stroke = outline(r.outline.unwrap_or(r.divider));
    // Under the mouse.
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, r.accent);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, r.strong_text);
    // Being pressed.
    v.widgets.active.bg_stroke = Stroke::new(1.0, r.accent);
    v.widgets.active.fg_stroke = Stroke::new(2.0, r.strong_text);
    // An open menu or dropdown.
    v.widgets.open.bg_stroke = Stroke::new(1.0, r.accent);
    v.widgets.open.fg_stroke = Stroke::new(1.0, r.strong_text);
    style.visuals = v;

    style.spacing.item_spacing = r.item_spacing;
    style.spacing.button_padding = r.button_padding;
    style.spacing.interact_size.y = r.row_height;
    style.spacing.menu_margin = egui::Margin::same(6);

    let size = r.text_size;
    for (text_style, font) in style.text_styles.iter_mut() {
        *font = match text_style {
            TextStyle::Heading => FontId::proportional(size * 1.4),
            TextStyle::Small => FontId::proportional(size * 0.8),
            TextStyle::Monospace => FontId::monospace(size),
            _ => FontId::proportional(size),
        };
    }
    style
}

/// Makes the whole app, including separate windows, use an appearance.
pub fn apply(ctx: &egui::Context, appearance: Appearance, dark: bool) {
    ctx.set_theme(if dark { egui::Theme::Dark } else { egui::Theme::Light });
    let style = style(appearance, dark);
    ctx.all_styles_mut(|s| *s = style.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Contrast ratio between two colours, as in the WCAG guidelines.
    fn contrast(a: Color32, b: Color32) -> f32 {
        fn luminance(c: Color32) -> f32 {
            let channel = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
        }
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn text_is_readable_in_every_appearance_and_mode() {
        for appearance in Appearance::ALL {
            for dark in [true, false] {
                let r = recipe(appearance, dark);
                let name = format!("{} ({})", appearance.title(), if dark { "dark" } else { "light" });
                for (what, text, background) in [
                    ("text on panels", r.text, r.panel),
                    ("text on windows", r.text, r.window),
                    ("text on buttons", r.text, r.button),
                    ("text on hovered buttons", r.strong_text, r.button_hovered),
                    ("text in text boxes", r.text, r.well),
                    ("text on the accent", r.on_accent, r.accent),
                    ("warnings on panels", r.warning, r.panel),
                ] {
                    let ratio = contrast(text, background);
                    assert!(ratio >= 4.5, "{name}: {what} has contrast {ratio:.2}, needs 4.5");
                }
                // Controls must stand out from the panel they sit on.
                assert_ne!(r.button, r.panel, "{name}");
            }
        }
    }

    #[test]
    fn darkroom_uses_only_neutral_greys_for_surfaces() {
        for dark in [true, false] {
            let r = recipe(Appearance::Darkroom, dark);
            for c in [r.panel, r.window, r.well, r.button, r.button_hovered, r.button_pressed, r.divider, r.text] {
                assert!(c.r() == c.g() && c.g() == c.b(), "{c:?} is not neutral");
            }
        }
    }

    #[test]
    fn every_appearance_builds_a_style_for_both_modes() {
        for appearance in Appearance::ALL {
            assert!(style(appearance, true).visuals.dark_mode);
            assert!(!style(appearance, false).visuals.dark_mode);
            assert!(!appearance.title().is_empty() && !appearance.description().is_empty());
        }
    }
}
