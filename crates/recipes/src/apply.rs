//! Turning a recipe into changes to the camera's profile, and a profile back
//! into recipe fields.
//!
//! Only the cells a recipe mentions are changed; everything else keeps the
//! value the photo was shot with.

use crate::settings::*;
use xrp_camera::profile::{cell, Profile};

pub struct Applied {
    pub profile: Profile,
    /// Things the user should know: settings skipped or adjusted.
    pub notes: Vec<String>,
}

/// Returns `as_shot` with the recipe's settings written in.
pub fn apply(settings: &Settings, as_shot: &Profile) -> Applied {
    let mut p = as_shot.clone();
    let mut notes = Vec::new();

    // Sets a cell, noting it if this camera's profile has no such setting.
    let mut set = |p: &mut Profile, index: usize, value: i32, what: &str| {
        if !p.set(index, value) {
            notes.push(format!("{what} is not available on this camera; skipped."));
        }
    };

    if let Some(code) = settings.film_simulation {
        set(&mut p, cell::FILM_SIMULATION, code, "Film simulation");
    }
    let film = p.get(cell::FILM_SIMULATION).unwrap_or(0);
    let mono = is_monochrome(film);

    if let Some(thirds) = settings.exposure_thirds {
        // The camera wants EV × 1000, rounded: +⅔ is 667.
        let value = (thirds as f32 * 1000.0 / 3.0).round() as i32;
        set(&mut p, cell::EXPOSURE_BIAS, value, "Exposure");
    }

    let mut later_notes = Vec::new();
    if let Some(wanted) = settings.dynamic_range {
        // A dynamic range above what the photo was shot with comes out wrong
        // (a green cast is documented), so it is capped.
        let shot = as_shot.get(cell::DYNAMIC_RANGE).unwrap_or(100);
        if [100, 200, 400].contains(&shot) && wanted > shot {
            later_notes.push(format!(
                "Dynamic range DR{wanted} needs a photo shot at DR{wanted} or higher; kept DR{shot}."
            ));
        } else {
            set(&mut p, cell::DYNAMIC_RANGE, wanted, "Dynamic range");
        }
    }

    if let Some(g) = settings.grain {
        let value = match (g.strength, g.large) {
            (Level::Off, _) => 1,
            (Level::Weak, false) => 2,
            (Level::Strong, false) => 3,
            (Level::Weak, true) => 4,
            (Level::Strong, true) => 5,
        };
        set(&mut p, cell::GRAIN, value, "Grain");
    }
    if let Some(l) = settings.color_chrome {
        set(&mut p, cell::COLOR_CHROME, l.code(), "Color Chrome");
    }
    if let Some(l) = settings.color_chrome_blue {
        set(&mut p, cell::COLOR_CHROME_BLUE, l.code(), "Color Chrome FX Blue");
    }
    if let Some(l) = settings.smooth_skin {
        set(&mut p, cell::SMOOTH_SKIN, l.code(), "Smooth skin");
    }

    match settings.white_balance {
        Some(WhiteBalance::AsShot) => set(&mut p, cell::WB_SOURCE, 1, "White balance"),
        Some(WhiteBalance::Mode(code)) => {
            set(&mut p, cell::WB_SOURCE, 2, "White balance");
            set(&mut p, cell::WHITE_BALANCE, code, "White balance");
        }
        Some(WhiteBalance::Kelvin(k)) => {
            set(&mut p, cell::WB_SOURCE, 2, "White balance");
            set(&mut p, cell::WHITE_BALANCE, WB_TEMPERATURE, "White balance");
            set(&mut p, cell::COLOR_TEMPERATURE, k, "Colour temperature");
        }
        None => {}
    }
    if let Some(v) = settings.wb_shift_red {
        set(&mut p, cell::WB_SHIFT_RED, v.clamp(-9, 9), "WB shift");
    }
    if let Some(v) = settings.wb_shift_blue {
        set(&mut p, cell::WB_SHIFT_BLUE, v.clamp(-9, 9), "WB shift");
    }

    if let Some(v) = settings.highlight {
        set(&mut p, cell::HIGHLIGHT, v, "Highlight");
    }
    if let Some(v) = settings.shadow {
        set(&mut p, cell::SHADOW, v, "Shadow");
    }
    if let Some(v) = settings.color {
        // Colour saturation has no meaning for a black-and-white simulation.
        if !mono {
            set(&mut p, cell::COLOR, v, "Color");
        }
    }
    if let Some(v) = settings.sharpness {
        set(&mut p, cell::SHARPNESS, v, "Sharpness");
    }
    if let Some(v) = settings.clarity {
        set(&mut p, cell::CLARITY, v, "Clarity");
    }
    if let Some(menu) = settings.noise_reduction {
        if let Some((_, code)) = NOISE_REDUCTION.iter().find(|(m, _)| *m == menu) {
            set(&mut p, cell::NOISE_REDUCTION, *code, "Noise reduction");
        }
    }
    if let Some(on) = settings.lens_optimizer {
        set(&mut p, cell::LENS_OPTIMIZER, if on { 1 } else { 2 }, "Lens modulation optimizer");
    }
    if let Some(adobe) = settings.adobe_rgb {
        set(&mut p, cell::COLOR_SPACE, if adobe { 2 } else { 1 }, "Colour space");
    }

    // Monochromatic colour only applies to black-and-white simulations; on
    // colour photos the camera uses those cells for something else.
    if mono {
        if let Some(v) = settings.mono_warm_cool {
            set(&mut p, cell::MONO_WARM_COOL, v.clamp(-180, 180), "Monochromatic colour");
        }
        if let Some(v) = settings.mono_magenta_green {
            set(&mut p, cell::MONO_MAGENTA_GREEN, v.clamp(-180, 180), "Monochromatic colour");
        }
    }

    notes.extend(later_notes);
    for bad in &settings.not_understood {
        notes.push(format!("Not understood, left as shot: {bad}."));
    }
    notes.dedup();
    Applied { profile: p, notes }
}

/// Names for the cells a recipe can change, for messages.
pub fn cell_name(index: usize) -> &'static str {
    match index {
        cell::EXPOSURE_BIAS => "Exposure",
        cell::DYNAMIC_RANGE => "Dynamic range",
        cell::FILM_SIMULATION => "Film simulation",
        cell::GRAIN => "Grain",
        cell::COLOR_CHROME => "Color Chrome",
        cell::WB_SOURCE | cell::WHITE_BALANCE => "White balance",
        cell::WB_SHIFT_RED | cell::WB_SHIFT_BLUE => "WB shift",
        cell::COLOR_TEMPERATURE => "Colour temperature",
        cell::HIGHLIGHT => "Highlight",
        cell::SHADOW => "Shadow",
        cell::COLOR => "Color",
        cell::SHARPNESS => "Sharpness",
        cell::NOISE_REDUCTION => "Noise reduction",
        cell::LENS_OPTIMIZER => "Lens modulation optimizer",
        cell::COLOR_SPACE => "Colour space",
        cell::MONO_WARM_COOL | cell::MONO_MAGENTA_GREEN => "Monochromatic colour",
        cell::SMOOTH_SKIN => "Smooth skin",
        cell::COLOR_CHROME_BLUE => "Color Chrome FX Blue",
        cell::CLARITY => "Clarity",
        _ => "A setting",
    }
}

/// Recipe fields describing a profile, in the order X RAW Studio writes them.
/// Used to save a photo's as-shot look as a new recipe.
pub fn fields_from_profile(p: &Profile) -> Vec<(String, String)> {
    let mut f: Vec<(String, String)> = Vec::new();
    let mut put = |tag: &str, value: String| f.push((tag.to_string(), value));
    let level = |index: usize| {
        p.get(index).and_then(Level::from_code).map(|l| l.word().to_string()).unwrap_or_default()
    };
    let tenths = |index: usize| p.get(index).map(tenths_word).unwrap_or_default();

    put("IOPCode", p.code());
    put("ShootingCondition", "OFF".into());
    put("FileType", "JPG".into());
    put("ImageSize", "L3x2".into());
    put("ImageQuality", "Fine".into());
    let thirds = p.get(cell::EXPOSURE_BIAS).map(|v| (v as f32 * 3.0 / 1000.0).round() as i32);
    put("ExposureBias", thirds.map(exposure_word).unwrap_or_default());
    put("DynamicRange", p.get(cell::DYNAMIC_RANGE).filter(|v| [100, 200, 400].contains(v)).map(|v| v.to_string()).unwrap_or_default());
    put("WideDRange", "0".into());
    let film = p.get(cell::FILM_SIMULATION).unwrap_or(0);
    put(
        "FilmSimulation",
        FILM_SIMULATIONS.iter().find(|(c, _, _)| *c == film).map(|(_, w, _)| w.to_string()).unwrap_or_default(),
    );
    let mono = is_monochrome(film);
    put("BlackImageTone", if mono { tenths(cell::MONO_WARM_COOL) } else { "0".into() });
    put("MonochromaticColor_RG", if mono { tenths(cell::MONO_MAGENTA_GREEN) } else { "0".into() });
    // Grain: 1, 6 and 7 all mean off; 2..5 are strength and size together.
    let (grain, size) = match p.get(cell::GRAIN) {
        Some(2) => ("WEAK", "SMALL"),
        Some(3) => ("STRONG", "SMALL"),
        Some(4) => ("WEAK", "LARGE"),
        Some(5) => ("STRONG", "LARGE"),
        Some(7) => ("OFF", "LARGE"),
        Some(_) => ("OFF", "SMALL"),
        None => ("", ""),
    };
    put("GrainEffect", grain.into());
    put("GrainEffectSize", size.into());
    put("ChromeEffect", level(cell::COLOR_CHROME));
    put("ColorChromeBlue", level(cell::COLOR_CHROME_BLUE));
    put("SmoothSkinEffect", level(cell::SMOOTH_SKIN));

    let mode = p.get(cell::WHITE_BALANCE).unwrap_or(0);
    let word = WHITE_BALANCES.iter().find(|(c, _, _)| *c == mode).map(|(_, w, _)| *w);
    match (p.get(cell::WB_SOURCE), word) {
        (Some(2), Some(word)) => {
            put("WBShootCond", "OFF".into());
            put("WhiteBalance", word.into());
        }
        _ => {
            put("WBShootCond", "ON".into());
            put("WhiteBalance", "INVALID".into());
        }
    }
    put("WBShiftR", p.get(cell::WB_SHIFT_RED).unwrap_or(0).to_string());
    put("WBShiftB", p.get(cell::WB_SHIFT_BLUE).unwrap_or(0).to_string());
    let kelvin = if mode == WB_TEMPERATURE { p.get(cell::COLOR_TEMPERATURE).unwrap_or(0) } else { 0 };
    put("WBColorTemp", format!("{kelvin}K"));

    put("HighlightTone", tenths(cell::HIGHLIGHT));
    put("ShadowTone", tenths(cell::SHADOW));
    put("Color", tenths(cell::COLOR));
    put("Sharpness", tenths(cell::SHARPNESS));
    let nr = p.get(cell::NOISE_REDUCTION);
    put(
        "NoisReduction",
        NOISE_REDUCTION.iter().find(|(_, c)| Some(*c) == nr).map(|(m, _)| m.to_string()).unwrap_or_default(),
    );
    put("Clarity", tenths(cell::CLARITY));
    put(
        "LensModulationOpt",
        match p.get(cell::LENS_OPTIMIZER) {
            Some(1) => "ON".into(),
            Some(2) => "OFF".into(),
            _ => String::new(),
        },
    );
    put(
        "ColorSpace",
        match p.get(cell::COLOR_SPACE) {
            Some(1) => "sRGB".into(),
            Some(2) => "AdobeRGB".into(),
            _ => String::new(),
        },
    );
    put("HDR", String::new());
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fp1::Recipe;

    /// A profile shaped like the X-T4's: 28 cells, as shot with Provia.
    fn as_shot() -> Profile {
        let mut p = Profile::from_bytes(vec![0u8; 621]).unwrap();
        p.set(cell::DYNAMIC_RANGE, 100);
        p.set(cell::FILM_SIMULATION, 1);
        p.set(cell::GRAIN, 6);
        p.set(cell::COLOR_CHROME, 1);
        p.set(cell::WB_SOURCE, 1);
        p.set(cell::NOISE_REDUCTION, 0x2000);
        p.set(cell::LENS_OPTIMIZER, 1);
        p.set(cell::COLOR_SPACE, 1);
        p.set(cell::COLOR_CHROME_BLUE, 1);
        p
    }

    fn recipe(body: &str) -> Settings {
        let text = format!("<ConversionProfile><PropertyGroup label=\"T\">{body}</PropertyGroup></ConversionProfile>");
        Settings::from_recipe(&Recipe::parse(&text).unwrap())
    }

    #[test]
    fn changes_only_what_the_recipe_mentions() {
        let s = recipe("<FilmSimulation>Classic</FilmSimulation><HighlightTone>-1.5</HighlightTone>");
        let base = as_shot();
        let applied = apply(&s, &base);
        let changed: Vec<usize> = base.differences(&applied.profile).iter().map(|d| d.0).collect();
        assert_eq!(changed, vec![cell::FILM_SIMULATION, cell::HIGHLIGHT]);
        assert_eq!(applied.profile.get(cell::FILM_SIMULATION), Some(0x0B));
        assert_eq!(applied.profile.get(cell::HIGHLIGHT), Some(-15));
        assert!(applied.notes.is_empty());
    }

    #[test]
    fn encodes_values_the_way_the_camera_expects() {
        let s = recipe(
            "<ExposureBias>P0P67</ExposureBias><GrainEffect>STRONG</GrainEffect><GrainEffectSize>LARGE</GrainEffectSize>\
             <ChromeEffect>STRONG</ChromeEffect><ColorChromeBlue>WEAK</ColorChromeBlue>\
             <WBShootCond>OFF</WBShootCond><WhiteBalance>Temperature</WhiteBalance><WBColorTemp>5900K</WBColorTemp>\
             <WBShiftR>2</WBShiftR><WBShiftB>-4</WBShiftB><NoisReduction>-4</NoisReduction>\
             <Clarity>3</Clarity><LensModulationOpt>OFF</LensModulationOpt><ColorSpace>AdobeRGB</ColorSpace>",
        );
        let p = apply(&s, &as_shot()).profile;
        assert_eq!(p.get(cell::EXPOSURE_BIAS), Some(667));
        assert_eq!(p.get(cell::GRAIN), Some(5));
        assert_eq!(p.get(cell::COLOR_CHROME), Some(3));
        assert_eq!(p.get(cell::COLOR_CHROME_BLUE), Some(2));
        assert_eq!(p.get(cell::WB_SOURCE), Some(2));
        assert_eq!(p.get(cell::WHITE_BALANCE), Some(0x8007));
        assert_eq!(p.get(cell::COLOR_TEMPERATURE), Some(5900));
        assert_eq!((p.get(cell::WB_SHIFT_RED), p.get(cell::WB_SHIFT_BLUE)), (Some(2), Some(-4)));
        assert_eq!(p.get(cell::NOISE_REDUCTION), Some(0x8000));
        assert_eq!(p.get(cell::CLARITY), Some(30));
        assert_eq!(p.get(cell::LENS_OPTIMIZER), Some(2));
        assert_eq!(p.get(cell::COLOR_SPACE), Some(2));
    }

    #[test]
    fn dynamic_range_is_capped_at_what_the_photo_was_shot_with() {
        let applied = apply(&recipe("<DynamicRange>400</DynamicRange>"), &as_shot());
        assert_eq!(applied.profile.get(cell::DYNAMIC_RANGE), Some(100));
        assert_eq!(applied.notes.len(), 1);
        let mut shot_400 = as_shot();
        shot_400.set(cell::DYNAMIC_RANGE, 400);
        let applied = apply(&recipe("<DynamicRange>200</DynamicRange>"), &shot_400);
        assert_eq!(applied.profile.get(cell::DYNAMIC_RANGE), Some(200));
        assert!(applied.notes.is_empty());
    }

    #[test]
    fn black_and_white_recipes_skip_colour_and_use_toning() {
        let s = recipe("<FilmSimulation>Acros</FilmSimulation><Color>4</Color><BlackImageTone>2</BlackImageTone>");
        let p = apply(&s, &as_shot()).profile;
        assert_eq!(p.get(cell::COLOR), Some(0), "colour is left alone");
        assert_eq!(p.get(cell::MONO_WARM_COOL), Some(20));
        // On a colour simulation the toning cells are not touched.
        let s = recipe("<FilmSimulation>Classic</FilmSimulation><BlackImageTone>-1</BlackImageTone>");
        assert_eq!(apply(&s, &as_shot()).profile.get(cell::MONO_WARM_COOL), Some(0));
    }

    #[test]
    fn settings_a_camera_lacks_are_noted_not_written() {
        // An older body with 24 cells has no Clarity cell.
        let mut old = Profile::from_bytes(vec![0u8; 605]).unwrap();
        old.set(cell::FILM_SIMULATION, 1);
        let applied = apply(&recipe("<Clarity>3</Clarity>"), &old);
        assert_eq!(applied.profile, old);
        assert_eq!(applied.notes, vec!["Clarity is not available on this camera; skipped.".to_string()]);
    }

    #[test]
    fn a_profile_saved_as_a_recipe_applies_back_to_the_same_profile() {
        let mut shot = as_shot();
        shot.set(cell::FILM_SIMULATION, 0x11);
        shot.set(cell::GRAIN, 4);
        shot.set(cell::WB_SOURCE, 2);
        shot.set(cell::WHITE_BALANCE, 0x8007);
        shot.set(cell::COLOR_TEMPERATURE, 6300);
        shot.set(cell::WB_SHIFT_RED, 3);
        shot.set(cell::HIGHLIGHT, -15);
        shot.set(cell::SHADOW, 20);
        shot.set(cell::COLOR, 40);
        shot.set(cell::SHARPNESS, -20);
        shot.set(cell::NOISE_REDUCTION, 0x8000);
        shot.set(cell::CLARITY, 30);
        shot.set(cell::EXPOSURE_BIAS, 333);
        let text = Recipe::build("Saved", "X-T4", "", &fields_from_profile(&shot));
        let settings = Settings::from_recipe(&Recipe::parse(&text).unwrap());
        assert!(settings.not_understood.is_empty(), "{:?}", settings.not_understood);
        // Applied to a plain photo, it reproduces the saved look.
        let mut plain = as_shot();
        plain.set(cell::GRAIN, 7);
        let applied = apply(&settings, &plain);
        let differences: Vec<_> = applied.profile.differences(&shot);
        // Grain "off" has two stored forms (6 and 7); nothing else may differ.
        assert!(differences.is_empty(), "{differences:?}");
    }
}
