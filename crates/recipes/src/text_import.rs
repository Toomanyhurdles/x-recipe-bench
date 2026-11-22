//! Reading a recipe written out as plain text, the way recipes are usually
//! shared online:
//!
//! ```text
//! Classic Chrome
//! Grain Effect: Strong, Small
//! Color Chrome Effect: Strong
//! White Balance: Daylight, +3 Red & -5 Blue
//! Highlight: -1
//! ```
//!
//! The reader is forgiving: it takes what it understands and lists the rest.

use crate::settings::*;

pub struct Imported {
    pub settings: Settings,
    /// Lines that looked like settings but could not be understood.
    pub skipped: Vec<String>,
    /// How many settings were understood.
    pub understood: usize,
}

/// The first signed number in `text`, in tenths: "+1.5" → 15.
fn number_tenths(text: &str) -> Option<i32> {
    let cleaned = text.replace(['−', '–'], "-");
    let start = cleaned.find(|c: char| c.is_ascii_digit())?;
    let negative = cleaned[..start].trim_end().ends_with('-');
    let digits: String = cleaned[start..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let value: f32 = digits.trim_end_matches('.').parse().ok()?;
    Some(((if negative { -value } else { value }) * 10.0).round() as i32)
}

fn level_in(text: &str) -> Option<Level> {
    let n = normalise(text);
    if n.contains("off") {
        Some(Level::Off)
    } else if n.contains("weak") {
        Some(Level::Weak)
    } else if n.contains("strong") {
        Some(Level::Strong)
    } else {
        None
    }
}

/// Finds a film simulation named anywhere in `text`, preferring longer names
/// so "Classic Negative" is not read as "Classic Chrome".
fn film_simulation_in(text: &str) -> Option<i32> {
    let n = normalise(text);
    const NAMES: &[(&str, i32)] = &[
        ("eternableachbypass", 0x12),
        ("bleachbypass", 0x12),
        ("nostalgicneg", 0x13),
        ("classicneg", 0x11),
        ("classicchrome", 0x0B),
        ("realaace", 0x14),
        ("reala", 0x14),
        ("pronegativehi", 0x04),
        ("proneghi", 0x04),
        ("pronegativestd", 0x05),
        ("pronegstd", 0x05),
        ("acrosye", 0x0D),
        ("acrosyellow", 0x0D),
        ("acrosr", 0x0E),
        ("acrosred", 0x0E),
        ("acrosg", 0x0F),
        ("acrosgreen", 0x0F),
        ("acros", 0x0C),
        ("monochromeye", 0x07),
        ("monochromeyellow", 0x07),
        ("monochromer", 0x08),
        ("monochromered", 0x08),
        ("monochromeg", 0x09),
        ("monochromegreen", 0x09),
        ("monochrome", 0x06),
        ("sepia", 0x0A),
        ("eterna", 0x10),
        ("velvia", 0x02),
        ("astia", 0x03),
        ("provia", 0x01),
    ];
    NAMES.iter().find(|(name, _)| n.contains(name)).map(|(_, code)| *code)
}

/// Reads "+3 Red & -5 Blue" (or "R: 3, B: -5") into (red, blue).
fn wb_shift(text: &str) -> (Option<i32>, Option<i32>) {
    let lower = text.to_lowercase().replace(['−', '–'], "-");
    let near = |word: &str, short: &str| -> Option<i32> {
        // Number just before "red"/"blue"...
        if let Some(at) = lower.find(word) {
            let before = &lower[..at];
            let start = before.rfind(|c: char| !(c.is_ascii_digit() || c == '+' || c == '-' || c == ' ')).map(|i| i + 1).unwrap_or(0);
            if let Ok(v) = before[start..].replace([' ', '+'], "").parse::<i32>() {
                return Some(v);
            }
        }
        // ...or just after "r:"/"b:".
        for marker in [format!("{short}:"), format!("{short} ")] {
            if let Some(at) = lower.find(&marker) {
                let after = lower[at + marker.len()..].trim_start();
                let digits: String = after
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '+' || *c == '-')
                    .collect();
                if let Ok(v) = digits.replace('+', "").parse::<i32>() {
                    return Some(v);
                }
            }
        }
        None
    };
    (near("red", "r"), near("blue", "b"))
}

pub fn parse(text: &str) -> Imported {
    let mut s = Settings::default();
    let mut skipped = Vec::new();
    let mut understood = 0;

    for line in text.lines() {
        let line = line.trim().trim_start_matches(['-', '*', '•']).trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = match line.split_once(':') {
            Some((k, v)) => (normalise(k), v.trim()),
            None => (String::new(), line),
        };
        let mut ok = true;
        match key.as_str() {
            "" | "filmsimulation" | "filmsim" | "simulation" => match film_simulation_in(value) {
                Some(code) => s.film_simulation = Some(code),
                // A line without a colon that is not a film simulation is just prose.
                None if key.is_empty() => continue,
                None => ok = false,
            },
            "graineffect" | "grain" => match level_in(value) {
                Some(strength) => {
                    s.grain = Some(Grain { strength, large: normalise(value).contains("large") })
                }
                None => ok = false,
            },
            "colorchromeeffect" | "colorchrome" | "colourchromeeffect" => match level_in(value) {
                Some(l) => s.color_chrome = Some(l),
                None => ok = false,
            },
            "colorchromefxblue" | "colorchromeeffectblue" | "colorchromeblue" | "ccfxblue" => match level_in(value) {
                Some(l) => s.color_chrome_blue = Some(l),
                None => ok = false,
            },
            "whitebalance" | "wb" => {
                let n = normalise(value);
                let kelvin = value
                    .to_lowercase()
                    .find('k')
                    .and_then(|at| {
                        let digits: String = value[..at].chars().rev().take_while(|c| c.is_ascii_digit() || *c == ' ').collect();
                        digits.chars().rev().collect::<String>().trim().parse::<i32>().ok()
                    })
                    .filter(|k| (2000..=12000).contains(k));
                let mode = if let Some(k) = kelvin {
                    Some(WhiteBalance::Kelvin(k))
                } else if n.starts_with("autowhitepriority") {
                    Some(WhiteBalance::Mode(0x8020))
                } else if n.starts_with("autoambience") {
                    Some(WhiteBalance::Mode(0x8021))
                } else if n.starts_with("auto") {
                    Some(WhiteBalance::Mode(0x0002))
                } else if n.starts_with("daylight") {
                    Some(WhiteBalance::Mode(0x0004))
                } else if n.starts_with("shade") {
                    Some(WhiteBalance::Mode(0x8006))
                } else if n.starts_with("incandescent") || n.starts_with("tungsten") {
                    Some(WhiteBalance::Mode(0x0006))
                } else if n.starts_with("underwater") {
                    Some(WhiteBalance::Mode(0x0008))
                } else if n.starts_with("fluorescent") {
                    let which = n.chars().find(|c| ('1'..='3').contains(c)).unwrap_or('1');
                    Some(WhiteBalance::Mode(0x8000 + which.to_digit(10).unwrap() as i32))
                } else {
                    None
                };
                match mode {
                    Some(m) => {
                        s.white_balance = Some(m);
                        let (r, b) = wb_shift(value);
                        s.wb_shift_red = r.map(|v| v.clamp(-9, 9)).or(s.wb_shift_red);
                        s.wb_shift_blue = b.map(|v| v.clamp(-9, 9)).or(s.wb_shift_blue);
                    }
                    None => ok = false,
                }
            }
            "whitebalanceshift" | "wbshift" => {
                let (r, b) = wb_shift(value);
                ok = r.is_some() || b.is_some();
                s.wb_shift_red = r.map(|v| v.clamp(-9, 9)).or(s.wb_shift_red);
                s.wb_shift_blue = b.map(|v| v.clamp(-9, 9)).or(s.wb_shift_blue);
            }
            "dynamicrange" | "dr" => {
                let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
                match digits.parse::<i32>().ok().filter(|v| [100, 200, 400].contains(v)) {
                    Some(v) => s.dynamic_range = Some(v),
                    None => ok = false, // "DR-Auto" and D-Range Priority are not recipe settings here
                }
            }
            "highlight" | "highlights" | "highlighttone" => match number_tenths(value) {
                Some(v) => s.highlight = Some(v.clamp(-20, 40)),
                None => ok = false,
            },
            "shadow" | "shadows" | "shadowtone" => match number_tenths(value) {
                Some(v) => s.shadow = Some(v.clamp(-20, 40)),
                None => ok = false,
            },
            "color" | "colour" => match number_tenths(value) {
                Some(v) => s.color = Some(v.clamp(-40, 40)),
                None => ok = false,
            },
            "sharpness" | "sharpening" => match number_tenths(value) {
                Some(v) => s.sharpness = Some(v.clamp(-40, 40)),
                None => ok = false,
            },
            "clarity" => match number_tenths(value) {
                Some(v) => s.clarity = Some(v.clamp(-50, 50)),
                None => ok = false,
            },
            "noisereduction" | "highisonr" | "nr" | "highisonoisereduction" => match number_tenths(value) {
                Some(v) => s.noise_reduction = Some((v / 10).clamp(-4, 4)),
                None => ok = false,
            },
            // Shooting advice, not a development setting.
            "iso" | "exposurecompensation" | "exposure" | "tonecurve" => continue,
            _ => ok = false,
        }
        if ok {
            understood += 1;
        } else {
            skipped.push(line.to_string());
        }
    }
    Imported { settings: s, skipped, understood }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_typical_shared_recipe() {
        let text = "My Warm Look\n\
            Film Simulation: Classic Chrome\n\
            Grain Effect: Strong, Small\n\
            Color Chrome Effect: Strong\n\
            Color Chrome FX Blue: Weak\n\
            White Balance: Daylight, +3 Red & -5 Blue\n\
            Dynamic Range: DR400\n\
            Highlight: -1\n\
            Shadow: +1.5\n\
            Color: +2\n\
            Sharpness: −2\n\
            High ISO NR: -4\n\
            Clarity: 0\n\
            ISO: Auto, up to ISO 6400\n\
            Exposure Compensation: +1/3 to +2/3 (typically)\n\
            Lens: any";
        let i = parse(text);
        let s = &i.settings;
        assert_eq!(s.film_simulation, Some(0x0B));
        assert_eq!(s.grain, Some(Grain { strength: Level::Strong, large: false }));
        assert_eq!(s.color_chrome, Some(Level::Strong));
        assert_eq!(s.color_chrome_blue, Some(Level::Weak));
        assert_eq!(s.white_balance, Some(WhiteBalance::Mode(0x0004)));
        assert_eq!((s.wb_shift_red, s.wb_shift_blue), (Some(3), Some(-5)));
        assert_eq!(s.dynamic_range, Some(400));
        assert_eq!((s.highlight, s.shadow, s.color), (Some(-10), Some(15), Some(20)));
        assert_eq!((s.sharpness, s.noise_reduction, s.clarity), (Some(-20), Some(-4), Some(0)));
        assert_eq!(i.understood, 12);
        assert_eq!(i.skipped, vec!["Lens: any".to_string()]);
    }

    #[test]
    fn reads_kelvin_bare_simulation_lines_and_similar_names() {
        let i = parse("Classic Negative\nWhite Balance: 5900K, +2 Red & -4 Blue\nGrain: Weak, Large");
        assert_eq!(i.settings.film_simulation, Some(0x11));
        assert_eq!(i.settings.white_balance, Some(WhiteBalance::Kelvin(5900)));
        assert_eq!((i.settings.wb_shift_red, i.settings.wb_shift_blue), (Some(2), Some(-4)));
        assert_eq!(i.settings.grain, Some(Grain { strength: Level::Weak, large: true }));
        assert!(i.skipped.is_empty());
        assert_eq!(parse("Film Simulation: Acros+R").settings.film_simulation, Some(0x0E));
        assert_eq!(parse("Film Simulation: PRO Neg. Hi").settings.film_simulation, Some(0x04));
        assert_eq!(parse("Film Simulation: Eterna Bleach Bypass").settings.film_simulation, Some(0x12));
        assert_eq!(parse("White Balance: Auto, 0 Red & 0 Blue").settings.wb_shift_blue, Some(0));
        assert_eq!(parse("just some words").understood, 0);
    }
}
