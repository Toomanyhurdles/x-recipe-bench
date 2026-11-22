//! What a recipe says, in typed form, and the words FP1 files use for it.
//!
//! Every setting is optional: a recipe that leaves one out keeps whatever the
//! photo was shot with.

use crate::fp1::Recipe;

/// (camera code, FP1 word, name shown to the user)
pub const FILM_SIMULATIONS: &[(i32, &str, &str)] = &[
    (0x01, "Provia", "PROVIA / Standard"),
    (0x02, "Velvia", "Velvia / Vivid"),
    (0x03, "Astia", "ASTIA / Soft"),
    (0x04, "NEGAhi", "PRO Neg. Hi"),
    (0x05, "NEGAStd", "PRO Neg. Std"),
    (0x06, "BW", "Monochrome"),
    (0x07, "BYe", "Monochrome + Ye"),
    (0x08, "BR", "Monochrome + R"),
    (0x09, "BG", "Monochrome + G"),
    (0x0A, "Sepia", "Sepia"),
    (0x0B, "Classic", "Classic Chrome"),
    (0x0C, "Acros", "ACROS"),
    (0x0D, "AcrosYe", "ACROS + Ye"),
    (0x0E, "AcrosR", "ACROS + R"),
    (0x0F, "AcrosG", "ACROS + G"),
    (0x10, "Eterna", "ETERNA / Cinema"),
    (0x11, "ClassicNEGA", "Classic Neg."),
    (0x12, "BleachBypass", "ETERNA Bleach Bypass"),
    (0x13, "NostalgicNEGA", "Nostalgic Neg."),
    (0x14, "RealaAce", "REALA ACE"),
];

/// Other spellings accepted when reading. The FP1 words for these two were
/// not seen in any sample file, so several likely forms are accepted.
const FILM_SIMULATION_ALIASES: &[(&str, i32)] = &[
    ("eternableachbypass", 0x12),
    ("eternabb", 0x12),
    ("bleach", 0x12),
    ("reala", 0x14),
    ("classicchrome", 0x0B),
    ("classicneg", 0x11),
    ("nostalgicneg", 0x13),
    ("monochrome", 0x06),
];

/// (camera code, FP1 word, name shown to the user)
pub const WHITE_BALANCES: &[(i32, &str, &str)] = &[
    (0x0002, "Auto", "Auto"),
    (0x0004, "Daylight", "Daylight"),
    (0x0006, "Incand", "Incandescent"),
    (0x0008, "Uwater", "Underwater"),
    (0x8001, "FLight1", "Fluorescent 1"),
    (0x8002, "FLight2", "Fluorescent 2"),
    (0x8003, "FLight3", "Fluorescent 3"),
    (0x8006, "Shade", "Shade"),
    (0x8007, "Temperature", "Colour temperature"),
    (0x8008, "Custom1", "Custom 1"),
    (0x8009, "Custom2", "Custom 2"),
    (0x800A, "Custom3", "Custom 3"),
    (0x8020, "AutoWhitePriority", "Auto, white priority"),
    (0x8021, "AutoAmbiencePriority", "Auto, ambience priority"),
];
pub const WB_TEMPERATURE: i32 = 0x8007;

/// High ISO noise reduction: menu value -4..=4 and the camera's code for it.
pub const NOISE_REDUCTION: &[(i32, i32)] = &[
    (-4, 0x8000),
    (-3, 0x7000),
    (-2, 0x4000),
    (-1, 0x3000),
    (0, 0x2000),
    (1, 0x1000),
    (2, 0x0000),
    (3, 0x6000),
    (4, 0x5000),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Level {
    Off,
    Weak,
    Strong,
}

impl Level {
    pub fn parse(word: &str) -> Option<Level> {
        match normalise(word).as_str() {
            "off" => Some(Level::Off),
            "weak" => Some(Level::Weak),
            "strong" => Some(Level::Strong),
            _ => None,
        }
    }
    pub fn word(self) -> &'static str {
        match self {
            Level::Off => "OFF",
            Level::Weak => "WEAK",
            Level::Strong => "STRONG",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Level::Off => "Off",
            Level::Weak => "Weak",
            Level::Strong => "Strong",
        }
    }
    /// The camera's code: 1 off, 2 weak, 3 strong.
    pub fn code(self) -> i32 {
        self as i32 + 1
    }
    pub fn from_code(code: i32) -> Option<Level> {
        [Level::Off, Level::Weak, Level::Strong].into_iter().find(|l| l.code() == code)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grain {
    pub strength: Level,
    pub large: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WhiteBalance {
    /// Keep the white balance the photo was shot with.
    AsShot,
    /// A camera white balance mode, by code.
    Mode(i32),
    Kelvin(i32),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    /// Camera code of the film simulation.
    pub film_simulation: Option<i32>,
    /// Exposure change in thirds of a stop (+1 = +1/3 EV).
    pub exposure_thirds: Option<i32>,
    /// 100, 200 or 400.
    pub dynamic_range: Option<i32>,
    pub grain: Option<Grain>,
    pub color_chrome: Option<Level>,
    pub color_chrome_blue: Option<Level>,
    pub smooth_skin: Option<Level>,
    pub white_balance: Option<WhiteBalance>,
    pub wb_shift_red: Option<i32>,
    pub wb_shift_blue: Option<i32>,
    /// Tone values in tenths, so +1.5 is 15.
    pub highlight: Option<i32>,
    pub shadow: Option<i32>,
    pub color: Option<i32>,
    pub sharpness: Option<i32>,
    pub clarity: Option<i32>,
    /// Menu value, -4..=4.
    pub noise_reduction: Option<i32>,
    pub lens_optimizer: Option<bool>,
    pub adobe_rgb: Option<bool>,
    /// Monochromatic colour, warm(+)/cool(-), in tenths.
    pub mono_warm_cool: Option<i32>,
    /// Monochromatic colour, green(+)/magenta(-), in tenths.
    pub mono_magenta_green: Option<i32>,
    /// Values in the file that could not be understood, for showing to the user.
    pub not_understood: Vec<String>,
}

/// Lower-case letters and digits only, so "PRO Neg. Hi" matches "pronegHi".
pub fn normalise(word: &str) -> String {
    word.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase()
}

pub fn film_simulation_code(word: &str) -> Option<i32> {
    let n = normalise(word);
    FILM_SIMULATIONS
        .iter()
        .find(|(_, fp, name)| normalise(fp) == n || normalise(name) == n)
        .map(|(code, _, _)| *code)
        .or_else(|| FILM_SIMULATION_ALIASES.iter().find(|(a, _)| *a == n).map(|(_, c)| *c))
}

pub fn film_simulation_name(code: i32) -> Option<&'static str> {
    FILM_SIMULATIONS.iter().find(|(c, _, _)| *c == code).map(|(_, _, name)| *name)
}

pub fn is_monochrome(film_simulation: i32) -> bool {
    matches!(film_simulation, 0x06..=0x09 | 0x0C..=0x0F)
}

/// "P0P33" → +1, "M0P67" → -2, "0" → 0 (thirds of a stop).
pub fn parse_exposure(word: &str) -> Option<i32> {
    let word = word.trim();
    if word == "0" {
        return Some(0);
    }
    let (sign, rest) = match word.chars().next()? {
        'P' | 'p' => (1.0, &word[1..]),
        'M' | 'm' => (-1.0, &word[1..]),
        _ => return None,
    };
    let (whole, fraction) = rest.split_once(['P', 'p'])?;
    let value: f32 = format!("{whole}.{fraction}").parse().ok()?;
    Some((sign * value * 3.0).round() as i32)
}

pub fn exposure_word(thirds: i32) -> String {
    if thirds == 0 {
        return "0".into();
    }
    let sign = if thirds > 0 { 'P' } else { 'M' };
    let whole = thirds.abs() / 3;
    let fraction = ["00", "33", "67"][(thirds.abs() % 3) as usize];
    format!("{sign}{whole}P{fraction}")
}

pub fn exposure_label(thirds: i32) -> String {
    if thirds == 0 {
        return "0 EV".into();
    }
    let sign = if thirds > 0 { '+' } else { '−' };
    let (whole, part) = (thirds.abs() / 3, thirds.abs() % 3);
    let fraction = ["", "⅓", "⅔"][part as usize];
    if whole == 0 {
        format!("{sign}{fraction} EV")
    } else {
        format!("{sign}{whole}{fraction} EV")
    }
}

/// "1.5" → 15, "-2" → -20.
fn parse_tenths(word: &str) -> Option<i32> {
    let v: f32 = word.trim().parse().ok()?;
    Some((v * 10.0).round() as i32)
}

pub fn tenths_word(tenths: i32) -> String {
    if tenths % 10 == 0 {
        format!("{}", tenths / 10)
    } else {
        format!("{:.1}", tenths as f32 / 10.0)
    }
}

fn signed_label(tenths: i32) -> String {
    let word = tenths_word(tenths.abs());
    match tenths {
        0 => "0".into(),
        t if t > 0 => format!("+{word}"),
        _ => format!("−{word}"),
    }
}

impl Settings {
    pub fn from_recipe(recipe: &Recipe) -> Settings {
        let mut s = Settings::default();
        let mut bad = Vec::new();

        // Reads one field with `parse`; notes values that cannot be understood.
        fn read<T>(
            recipe: &Recipe,
            tag: &str,
            bad: &mut Vec<String>,
            parse: impl Fn(&str) -> Option<T>,
        ) -> Option<T> {
            let word = recipe.field(tag)?;
            let value = parse(word);
            if value.is_none() {
                bad.push(format!("{tag}: {word}"));
            }
            value
        }

        s.film_simulation = read(recipe, "FilmSimulation", &mut bad, film_simulation_code);
        s.exposure_thirds = read(recipe, "ExposureBias", &mut bad, parse_exposure);
        s.dynamic_range = read(recipe, "DynamicRange", &mut bad, |w| {
            let digits: String = w.chars().filter(|c| c.is_ascii_digit()).collect();
            digits.parse().ok().filter(|v| [100, 200, 400].contains(v))
        });

        let strength = read(recipe, "GrainEffect", &mut bad, Level::parse);
        let large = recipe.field("GrainEffectSize").map(|w| normalise(w) == "large").unwrap_or(false);
        s.grain = strength.map(|strength| Grain { strength, large });

        s.color_chrome = read(recipe, "ChromeEffect", &mut bad, Level::parse);
        s.color_chrome_blue = read(recipe, "ColorChromeBlue", &mut bad, Level::parse);
        s.smooth_skin = read(recipe, "SmoothSkinEffect", &mut bad, Level::parse);

        // WBShootCond ON means "keep the white balance the photo was shot with".
        let as_shot = recipe.field("WBShootCond").map(|w| normalise(w) == "on").unwrap_or(false);
        if as_shot {
            s.white_balance = Some(WhiteBalance::AsShot);
        } else if let Some(word) = recipe.field("WhiteBalance") {
            let n = normalise(word);
            let code = WHITE_BALANCES.iter().find(|(_, fp, _)| normalise(fp) == n).map(|(c, _, _)| *c);
            match code {
                Some(WB_TEMPERATURE) => {
                    let kelvin = recipe
                        .field("WBColorTemp")
                        .and_then(|w| w.trim_end_matches(['K', 'k']).parse::<i32>().ok())
                        .filter(|k| (2000..=12000).contains(k));
                    match kelvin {
                        Some(k) => s.white_balance = Some(WhiteBalance::Kelvin(k)),
                        None => bad.push("WBColorTemp: missing for Temperature".into()),
                    }
                }
                Some(code) => s.white_balance = Some(WhiteBalance::Mode(code)),
                // "INVALID" is what X RAW Studio writes when there is nothing to say.
                None if n == "invalid" || n == "asshot" => {}
                None => bad.push(format!("WhiteBalance: {word}")),
            }
        }
        s.wb_shift_red = read(recipe, "WBShiftR", &mut bad, |w| w.trim().parse().ok());
        s.wb_shift_blue = read(recipe, "WBShiftB", &mut bad, |w| w.trim().parse().ok());

        s.highlight = read(recipe, "HighlightTone", &mut bad, parse_tenths);
        s.shadow = read(recipe, "ShadowTone", &mut bad, parse_tenths);
        s.color = read(recipe, "Color", &mut bad, parse_tenths);
        s.sharpness = read(recipe, "Sharpness", &mut bad, parse_tenths);
        s.clarity = read(recipe, "Clarity", &mut bad, parse_tenths);
        s.noise_reduction = read(recipe, "NoisReduction", &mut bad, |w| {
            w.trim().parse().ok().filter(|v| (-4..=4).contains(v))
        });
        s.lens_optimizer = read(recipe, "LensModulationOpt", &mut bad, |w| match normalise(w).as_str() {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        });
        s.adobe_rgb = read(recipe, "ColorSpace", &mut bad, |w| match normalise(w).as_str() {
            "srgb" => Some(false),
            "adobergb" => Some(true),
            _ => None,
        });
        s.mono_warm_cool = read(recipe, "BlackImageTone", &mut bad, parse_tenths);
        s.mono_magenta_green = read(recipe, "MonochromaticColor_RG", &mut bad, parse_tenths);

        s.not_understood = bad;
        s
    }

    /// True when the recipe changes nothing.
    pub fn is_empty(&self) -> bool {
        let blank = Settings { not_understood: self.not_understood.clone(), ..Default::default() };
        *self == blank
    }

    /// A number that differs whenever the settings differ.
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!("{self:?}").hash(&mut hasher);
        hasher.finish()
    }

    /// The settings as FP1 (tag, text) pairs. A setting that is not set gives
    /// an empty text, which FP1 readers treat as "leave alone".
    pub fn to_fields(&self) -> Vec<(String, String)> {
        let mut f: Vec<(String, String)> = Vec::new();
        let mut put = |tag: &str, value: Option<String>| f.push((tag.to_string(), value.unwrap_or_default()));
        let tenths = |v: Option<i32>| v.map(tenths_word);
        let level = |v: Option<Level>| v.map(|l| l.word().to_string());

        put("ExposureBias", self.exposure_thirds.map(exposure_word));
        put("DynamicRange", self.dynamic_range.map(|v| v.to_string()));
        put(
            "FilmSimulation",
            self.film_simulation
                .and_then(|code| FILM_SIMULATIONS.iter().find(|(c, _, _)| *c == code))
                .map(|(_, word, _)| word.to_string()),
        );
        put("BlackImageTone", tenths(self.mono_warm_cool));
        put("MonochromaticColor_RG", tenths(self.mono_magenta_green));
        put("GrainEffect", self.grain.map(|g| g.strength.word().to_string()));
        put("GrainEffectSize", self.grain.map(|g| if g.large { "LARGE" } else { "SMALL" }.to_string()));
        put("ChromeEffect", level(self.color_chrome));
        put("ColorChromeBlue", level(self.color_chrome_blue));
        put("SmoothSkinEffect", level(self.smooth_skin));
        let (cond, mode, kelvin) = match self.white_balance {
            None => (None, None, None),
            Some(WhiteBalance::AsShot) => (Some("ON"), Some("INVALID".to_string()), Some(0)),
            Some(WhiteBalance::Kelvin(k)) => (Some("OFF"), Some("Temperature".to_string()), Some(k)),
            Some(WhiteBalance::Mode(code)) => (
                Some("OFF"),
                WHITE_BALANCES.iter().find(|(c, _, _)| *c == code).map(|(_, w, _)| w.to_string()),
                Some(0),
            ),
        };
        put("WBShootCond", cond.map(str::to_string));
        put("WhiteBalance", mode);
        put("WBShiftR", self.wb_shift_red.map(|v| v.to_string()));
        put("WBShiftB", self.wb_shift_blue.map(|v| v.to_string()));
        put("WBColorTemp", kelvin.map(|k| format!("{k}K")));
        put("HighlightTone", tenths(self.highlight));
        put("ShadowTone", tenths(self.shadow));
        put("Color", tenths(self.color));
        put("Sharpness", tenths(self.sharpness));
        put("NoisReduction", self.noise_reduction.map(|v| v.to_string()));
        put("Clarity", tenths(self.clarity));
        put("LensModulationOpt", self.lens_optimizer.map(|on| if on { "ON" } else { "OFF" }.to_string()));
        put("ColorSpace", self.adobe_rgb.map(|a| if a { "AdobeRGB" } else { "sRGB" }.to_string()));
        f
    }

    /// The settings as (label, value) lines for showing to the user.
    /// Settings the recipe leaves alone are not listed.
    pub fn summary(&self) -> Vec<(&'static str, String)> {
        let mut lines = Vec::new();
        if let Some(code) = self.film_simulation {
            lines.push(("Film simulation", film_simulation_name(code).unwrap_or("?").to_string()));
        }
        if let Some(g) = self.grain {
            let text = match g.strength {
                Level::Off => "Off".to_string(),
                l => format!("{}, {}", l.label(), if g.large { "large" } else { "small" }),
            };
            lines.push(("Grain", text));
        }
        if let Some(l) = self.color_chrome {
            lines.push(("Color Chrome", l.label().into()));
        }
        if let Some(l) = self.color_chrome_blue {
            lines.push(("Color Chrome FX Blue", l.label().into()));
        }
        match self.white_balance {
            Some(WhiteBalance::AsShot) => lines.push(("White balance", "As shot".into())),
            Some(WhiteBalance::Kelvin(k)) => lines.push(("White balance", format!("{k} K"))),
            Some(WhiteBalance::Mode(code)) => {
                let name = WHITE_BALANCES.iter().find(|(c, _, _)| *c == code).map(|(_, _, n)| *n);
                lines.push(("White balance", name.unwrap_or("?").to_string()));
            }
            None => {}
        }
        let (r, b) = (self.wb_shift_red.unwrap_or(0), self.wb_shift_blue.unwrap_or(0));
        if r != 0 || b != 0 {
            lines.push(("WB shift", format!("R {r:+}, B {b:+}")));
        }
        if let Some(dr) = self.dynamic_range {
            lines.push(("Dynamic range", format!("DR{dr}")));
        }
        for (label, value) in [
            ("Highlight", self.highlight),
            ("Shadow", self.shadow),
            ("Color", self.color),
            ("Sharpness", self.sharpness),
            ("Clarity", self.clarity),
        ] {
            if let Some(v) = value {
                lines.push((label, signed_label(v)));
            }
        }
        if let Some(nr) = self.noise_reduction {
            lines.push(("Noise reduction", signed_label(nr * 10)));
        }
        if let Some(t) = self.exposure_thirds {
            if t != 0 {
                lines.push(("Exposure", exposure_label(t)));
            }
        }
        if let Some(l) = self.smooth_skin {
            lines.push(("Smooth skin", l.label().into()));
        }
        let mono = self.film_simulation.map(is_monochrome).unwrap_or(false);
        if mono {
            let (wc, mg) = (self.mono_warm_cool.unwrap_or(0), self.mono_magenta_green.unwrap_or(0));
            if wc != 0 || mg != 0 {
                lines.push(("Mono colour", format!("WC {}, MG {}", signed_label(wc), signed_label(mg))));
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposure_words() {
        assert_eq!(parse_exposure("0"), Some(0));
        assert_eq!(parse_exposure("P0P33"), Some(1));
        assert_eq!(parse_exposure("P0P67"), Some(2));
        assert_eq!(parse_exposure("M0P67"), Some(-2));
        assert_eq!(parse_exposure("P2P0"), Some(6));
        assert_eq!(parse_exposure("M1P33"), Some(-4));
        assert_eq!(parse_exposure("bright"), None);
        for t in -9..=9 {
            assert_eq!(parse_exposure(&exposure_word(t)), Some(t));
        }
        assert_eq!(exposure_label(4), "+1⅓ EV");
        assert_eq!(exposure_label(-2), "−⅔ EV");
    }

    #[test]
    fn film_simulation_words() {
        assert_eq!(film_simulation_code("Classic"), Some(0x0B));
        assert_eq!(film_simulation_code("ClassicNEGA"), Some(0x11));
        assert_eq!(film_simulation_code("NEGAhi"), Some(0x04));
        assert_eq!(film_simulation_code("BR"), Some(0x08));
        assert_eq!(film_simulation_code("AcrosYe"), Some(0x0D));
        assert_eq!(film_simulation_code("Kodachrome"), None);
        assert!(is_monochrome(0x0C) && is_monochrome(0x06) && !is_monochrome(0x0B) && !is_monochrome(0x0A));
    }

    #[test]
    fn reads_a_whole_recipe() {
        let text = r#"<ConversionProfile><PropertyGroup device="X-T3" label="T">
            <ExposureBias>P0P33</ExposureBias><DynamicRange>200</DynamicRange>
            <FilmSimulation>Classic</FilmSimulation>
            <GrainEffect>STRONG</GrainEffect><GrainEffectSize>LARGE</GrainEffectSize>
            <ChromeEffect>WEAK</ChromeEffect><ColorChromeBlue/>
            <WBShootCond>OFF</WBShootCond><WhiteBalance>Temperature</WhiteBalance>
            <WBShiftR>2</WBShiftR><WBShiftB>-4</WBShiftB><WBColorTemp>5900K</WBColorTemp>
            <HighlightTone>-1.5</HighlightTone><ShadowTone>2</ShadowTone><Color>4</Color>
            <Sharpness>-2</Sharpness><NoisReduction>-4</NoisReduction><Clarity>0</Clarity>
            <LensModulationOpt>ON</LensModulationOpt><ColorSpace>sRGB</ColorSpace>
            </PropertyGroup></ConversionProfile>"#;
        let s = Settings::from_recipe(&Recipe::parse(text).unwrap());
        assert_eq!(s.film_simulation, Some(0x0B));
        assert_eq!(s.exposure_thirds, Some(1));
        assert_eq!(s.dynamic_range, Some(200));
        assert_eq!(s.grain, Some(Grain { strength: Level::Strong, large: true }));
        assert_eq!(s.color_chrome, Some(Level::Weak));
        assert_eq!(s.color_chrome_blue, None);
        assert_eq!(s.white_balance, Some(WhiteBalance::Kelvin(5900)));
        assert_eq!((s.wb_shift_red, s.wb_shift_blue), (Some(2), Some(-4)));
        assert_eq!((s.highlight, s.shadow, s.color), (Some(-15), Some(20), Some(40)));
        assert_eq!((s.sharpness, s.noise_reduction, s.clarity), (Some(-20), Some(-4), Some(0)));
        assert_eq!((s.lens_optimizer, s.adobe_rgb), (Some(true), Some(false)));
        assert!(s.not_understood.is_empty(), "{:?}", s.not_understood);
        let summary = s.summary();
        assert!(summary.contains(&("Film simulation", "Classic Chrome".into())));
        assert!(summary.contains(&("Highlight", "−1.5".into())));
        assert!(summary.contains(&("White balance", "5900 K".into())));
    }

    #[test]
    fn settings_survive_being_written_to_a_file_and_read_back() {
        let s = Settings {
            film_simulation: Some(0x11),
            exposure_thirds: Some(-2),
            dynamic_range: Some(200),
            grain: Some(Grain { strength: Level::Weak, large: true }),
            color_chrome: Some(Level::Strong),
            color_chrome_blue: Some(Level::Off),
            white_balance: Some(WhiteBalance::Kelvin(6300)),
            wb_shift_red: Some(3),
            wb_shift_blue: Some(-5),
            highlight: Some(-15),
            shadow: Some(25),
            color: Some(40),
            sharpness: Some(-20),
            clarity: Some(-30),
            noise_reduction: Some(-4),
            ..Default::default()
        };
        let text = Recipe::build("Round trip", "X-T4", "", &s.to_fields());
        assert_eq!(Settings::from_recipe(&Recipe::parse(&text).unwrap()), s);

        for wb in [WhiteBalance::AsShot, WhiteBalance::Mode(0x0004), WhiteBalance::Mode(0x8006)] {
            let s = Settings { white_balance: Some(wb), ..Default::default() };
            let text = Recipe::build("WB", "", "", &s.to_fields());
            assert_eq!(Settings::from_recipe(&Recipe::parse(&text).unwrap()), s);
        }
        let blank = Settings::default();
        assert!(blank.is_empty());
        let text = Recipe::build("Blank", "", "", &blank.to_fields());
        assert!(Settings::from_recipe(&Recipe::parse(&text).unwrap()).is_empty());
        assert_ne!(s.fingerprint(), blank.fingerprint());
    }

    #[test]
    fn as_shot_white_balance_and_unknown_words() {
        let text = r#"<ConversionProfile><PropertyGroup label="T">
            <FilmSimulation>Kodachrome</FilmSimulation>
            <WBShootCond>ON</WBShootCond><WhiteBalance>INVALID</WhiteBalance><WBColorTemp>0K</WBColorTemp>
            </PropertyGroup></ConversionProfile>"#;
        let s = Settings::from_recipe(&Recipe::parse(text).unwrap());
        assert_eq!(s.white_balance, Some(WhiteBalance::AsShot));
        assert_eq!(s.film_simulation, None);
        assert_eq!(s.not_understood, vec!["FilmSimulation: Kodachrome".to_string()]);
    }
}
