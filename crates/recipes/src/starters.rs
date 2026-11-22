//! The recipes a new library starts with.
//!
//! These are original starting points written for X Recipe Bench, sized for an
//! X-Trans IV camera such as the X-T4. They have not been tuned against real
//! photos; they are meant to be tried, adjusted and replaced.

use crate::fp1::Recipe;
use crate::settings::{Grain, Level, Settings, WhiteBalance};

pub struct Starter {
    pub name: &'static str,
    pub description: &'static str,
    pub categories: &'static [&'static str],
    pub settings: Settings,
}

impl Starter {
    pub fn file_name(&self) -> String {
        format!("{}.FP1", self.name)
    }
    pub fn file_text(&self) -> String {
        Recipe::build(self.name, "X-T4", self.description, &self.settings.to_fields())
    }
}

const OFF: Level = Level::Off;
const WEAK: Level = Level::Weak;
const STRONG: Level = Level::Strong;

// Film simulation codes.
const PROVIA: i32 = 0x01;
const VELVIA: i32 = 0x02;
const ASTIA: i32 = 0x03;
const PRO_NEG_HI: i32 = 0x04;
const PRO_NEG_STD: i32 = 0x05;
const MONOCHROME: i32 = 0x06;
const SEPIA: i32 = 0x0A;
const CLASSIC_CHROME: i32 = 0x0B;
const ACROS: i32 = 0x0C;
const ACROS_R: i32 = 0x0E;
const ETERNA: i32 = 0x10;
const CLASSIC_NEG: i32 = 0x11;
const BLEACH_BYPASS: i32 = 0x12;

// White balance modes.
const AUTO: WhiteBalance = WhiteBalance::Mode(0x0002);
const DAYLIGHT: WhiteBalance = WhiteBalance::Mode(0x0004);
const INCANDESCENT: WhiteBalance = WhiteBalance::Mode(0x0006);
const SHADE: WhiteBalance = WhiteBalance::Mode(0x8006);

/// One starter. Tone values are in menu units: 1.5 means +1.5.
#[allow(clippy::too_many_arguments)]
fn starter(
    name: &'static str,
    description: &'static str,
    categories: &'static [&'static str],
    film: i32,
    grain: (Level, bool),
    chrome: (Level, Level),
    wb: (WhiteBalance, i32, i32),
    tone: (f32, f32),
    color: f32,
    sharpness: f32,
    noise_reduction: i32,
    clarity: f32,
) -> Starter {
    let tenths = |v: f32| Some((v * 10.0).round() as i32);
    Starter {
        name,
        description,
        categories,
        settings: Settings {
            film_simulation: Some(film),
            grain: Some(Grain { strength: grain.0, large: grain.1 }),
            color_chrome: Some(chrome.0),
            color_chrome_blue: Some(chrome.1),
            white_balance: Some(wb.0),
            wb_shift_red: Some(wb.1),
            wb_shift_blue: Some(wb.2),
            highlight: tenths(tone.0),
            shadow: tenths(tone.1),
            color: tenths(color),
            sharpness: tenths(sharpness),
            noise_reduction: Some(noise_reduction),
            clarity: tenths(clarity),
            ..Default::default()
        },
    }
}

const SMALL: bool = false;
const LARGE: bool = true;

pub fn all() -> Vec<Starter> {
    let k = WhiteBalance::Kelvin;
    vec![
        // Warm
        starter("Golden Hour Print", "Warm, slightly faded colour with soft highlights. Made for late-afternoon light.",
            &["Favorites", "Warm", "Nostalgia"], CLASSIC_CHROME, (WEAK, SMALL), (STRONG, WEAK), (DAYLIGHT, 3, -5), (-1.0, 1.0), 2.0, -1.0, -4, 0.0),
        starter("Campfire", "Deep amber cast with strong shadows. For firelight, candles and warm interiors.",
            &["Warm", "Night"], CLASSIC_CHROME, (STRONG, SMALL), (WEAK, OFF), (k(6300), 2, -2), (-1.0, 2.0), 3.0, 0.0, -3, 1.0),
        starter("Sunday Slides", "Gentle warm colour with open shadows. Flattering for people outdoors.",
            &["Warm", "Portrait"], ASTIA, (WEAK, SMALL), (OFF, OFF), (DAYLIGHT, 2, -3), (0.0, -1.0), 1.0, 0.0, -3, 0.0),
        // Nostalgia
        starter("Faded Postcard", "Washed-out colour and heavy grain, like a print left in the sun.",
            &["Nostalgia", "Muted"], CLASSIC_NEG, (STRONG, LARGE), (WEAK, OFF), (AUTO, 2, -4), (-1.5, -1.0), -1.0, -2.0, -4, -2.0),
        starter("Drugstore Print", "Punchy consumer-film colour with a warm tint and visible grain.",
            &["Nostalgia", "Film Emulation", "Warm"], CLASSIC_NEG, (STRONG, SMALL), (WEAK, OFF), (DAYLIGHT, 3, -4), (0.0, 1.0), 2.0, -1.0, -4, -1.0),
        starter("Sepia Album", "Brown-toned and soft, for an old family-album look.",
            &["Nostalgia", "Monochrome"], SEPIA, (WEAK, LARGE), (OFF, OFF), (AUTO, 0, 0), (-1.0, 0.0), 0.0, -2.0, -4, -1.0),
        // Cool
        starter("Glacier", "Clean, cold colour with deep blues. For snow, water and clear skies.",
            &["Cool", "Landscape"], CLASSIC_CHROME, (OFF, SMALL), (WEAK, STRONG), (k(4700), -2, 4), (1.0, 0.0), -1.0, 1.0, -2, 1.0),
        starter("Blue Hour", "Soft cinematic blue for the minutes after sunset.",
            &["Cool", "Night"], ETERNA, (WEAK, SMALL), (OFF, STRONG), (k(4300), -1, 3), (-1.0, 1.0), 1.0, 0.0, -2, 0.0),
        starter("Overcast Teal", "Muted green-blue shadows that suit flat, grey days.",
            &["Cool", "Muted", "Street"], PRO_NEG_STD, (WEAK, SMALL), (OFF, WEAK), (AUTO, -2, 2), (-1.0, -1.0), 0.0, 0.0, -3, 0.0),
        // Monochrome
        starter("Newsprint", "Hard, grainy black and white with strong contrast.",
            &["Favorites", "Monochrome", "Street"], ACROS, (STRONG, LARGE), (OFF, OFF), (AUTO, 0, 0), (2.0, 3.0), 0.0, 1.0, -4, 2.0),
        starter("Soft Silver", "Low-contrast black and white with smooth tones. Kind to faces.",
            &["Monochrome", "Portrait"], MONOCHROME, (WEAK, SMALL), (OFF, OFF), (AUTO, 0, 0), (-1.0, -1.0), 0.0, -1.0, -2, -2.0),
        starter("Red Filter Sky", "Black and white with darkened skies and crisp detail.",
            &["Monochrome", "Landscape"], ACROS_R, (OFF, SMALL), (OFF, OFF), (AUTO, 0, 0), (1.0, 2.0), 0.0, 2.0, -3, 3.0),
        // Portrait
        starter("Gentle Skin", "Low contrast and low sharpening for soft, natural skin.",
            &["Portrait", "Muted"], PRO_NEG_STD, (OFF, SMALL), (OFF, OFF), (AUTO, 1, -1), (-1.0, -1.0), 0.0, -2.0, 0, -2.0),
        starter("Studio Clean", "Neutral colour with a little extra snap. A plain, accurate starting point.",
            &["Portrait"], PRO_NEG_HI, (OFF, SMALL), (OFF, OFF), (AUTO, 0, 0), (0.0, 0.0), 1.0, 1.0, -2, 0.0),
        // Landscape
        starter("Big Valley", "Saturated colour and strong detail for wide scenery.",
            &["Favorites", "Landscape", "Vivid"], VELVIA, (OFF, SMALL), (STRONG, WEAK), (DAYLIGHT, 0, 0), (-1.0, -1.0), 2.0, 2.0, -3, 2.0),
        starter("Quiet Forest", "Soft greens and held-back highlights for woodland and shade.",
            &["Landscape"], ASTIA, (OFF, SMALL), (WEAK, OFF), (SHADE, -1, -1), (-1.0, 1.0), 1.0, 0.0, -2, 1.0),
        // Street
        starter("Pavement", "Gritty colour with firm contrast for city scenes.",
            &["Street", "Film Emulation"], CLASSIC_NEG, (WEAK, SMALL), (OFF, OFF), (AUTO, 1, -3), (1.0, 2.0), 0.0, 0.0, -4, 1.0),
        starter("Corner Shop", "Documentary-style colour: restrained, contrasty and grainy.",
            &["Street"], CLASSIC_CHROME, (STRONG, SMALL), (OFF, OFF), (AUTO, 0, 0), (1.0, 1.5), 1.0, 1.0, -4, 0.0),
        // Night
        starter("Neon Rain", "Cool base with saturated colour for signs and wet streets.",
            &["Night", "Vivid"], CLASSIC_NEG, (WEAK, SMALL), (OFF, STRONG), (k(3900), -2, 2), (-1.0, 1.0), 3.0, 0.0, -1, 0.0),
        starter("Tungsten Cinema", "Soft highlights and film-like colour under indoor lighting.",
            &["Night", "Film Emulation"], ETERNA, (WEAK, LARGE), (OFF, OFF), (INCANDESCENT, 1, -2), (-2.0, -1.0), 2.0, -1.0, -2, 0.0),
        // Vivid
        starter("Candy Shop", "As much colour as the camera will give.",
            &["Vivid"], VELVIA, (OFF, SMALL), (STRONG, STRONG), (AUTO, 0, 0), (0.0, 0.0), 4.0, 1.0, -2, 0.0),
        starter("Poster Paint", "Bold colour and contrast without the heaviness of Velvia.",
            &["Vivid"], PROVIA, (OFF, SMALL), (STRONG, OFF), (AUTO, 0, 0), (1.0, 1.0), 3.0, 1.0, -2, 2.0),
        // Muted
        starter("Paper Dust", "Pale, flat and quiet. Low colour and low contrast.",
            &["Muted"], ETERNA, (WEAK, SMALL), (OFF, OFF), (AUTO, 0, 0), (-1.0, -2.0), -2.0, -2.0, -3, -1.0),
        starter("Bleached Linen", "Desaturated with firm contrast, like a bleach-bypass print.",
            &["Muted", "Film Emulation"], BLEACH_BYPASS, (WEAK, SMALL), (OFF, OFF), (AUTO, 0, 0), (-1.0, 1.0), -1.0, 0.0, -3, 0.0),
        // Film Emulation
        starter("Slide Projector", "Rich, contrasty slide-film colour with fine grain.",
            &["Film Emulation", "Vivid"], VELVIA, (WEAK, SMALL), (WEAK, OFF), (DAYLIGHT, 1, -2), (1.0, 1.0), 1.0, 1.0, -3, 0.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::DEFAULT_CATEGORIES;

    #[test]
    fn every_starter_is_a_valid_recipe_within_the_cameras_ranges() {
        let starters = all();
        let mut names = std::collections::HashSet::new();
        for s in &starters {
            assert!(names.insert(s.name), "duplicate name {}", s.name);
            let recipe = Recipe::parse(&s.file_text()).unwrap();
            assert_eq!(recipe.name, s.name);
            assert_eq!(recipe.description, s.description);
            let read = Settings::from_recipe(&recipe);
            assert_eq!(read, s.settings, "{}", s.name);
            assert!(read.not_understood.is_empty());
            // X-T4 menu ranges.
            assert!((-20..=40).contains(&read.highlight.unwrap()), "{}", s.name);
            assert!((-20..=40).contains(&read.shadow.unwrap()), "{}", s.name);
            assert!((-40..=40).contains(&read.color.unwrap()), "{}", s.name);
            assert!((-40..=40).contains(&read.sharpness.unwrap()), "{}", s.name);
            assert!((-50..=50).contains(&read.clarity.unwrap()), "{}", s.name);
            assert!((-9..=9).contains(&read.wb_shift_red.unwrap()) && (-9..=9).contains(&read.wb_shift_blue.unwrap()));
            for c in s.categories {
                assert!(DEFAULT_CATEGORIES.contains(c), "{} uses unknown category {c}", s.name);
            }
        }
    }

    #[test]
    fn every_category_has_at_least_two_starters() {
        let starters = all();
        for category in DEFAULT_CATEGORIES {
            let count = starters.iter().filter(|s| s.categories.contains(category)).count();
            assert!(count >= 2, "{category} has only {count}");
        }
    }
}
