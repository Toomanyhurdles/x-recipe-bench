//! Recipes and the camera's custom slots: which numbers to write for a
//! recipe, in which order, and what a slot's numbers mean.
//!
//! A slot stores the same look settings a recipe has, except exposure
//! compensation. Only the settings a recipe sets are written; the rest of
//! the slot (and its image size, quality and so on) stays as it is.
//!
//! The camera is particular about order and state:
//! - D-Range Priority must be off before dynamic range, highlight or shadow
//!   can be stored, so it is switched off first when the recipe sets those.
//! - The colour temperature goes straight after the white balance mode, and
//!   the white balance shift after that (each mode keeps its own shift).
//! - Monochromatic colour is only accepted with a black-and-white film
//!   simulation, and Color only without one.
//!
//! Encodings and rules: zgrimz/fujifilm-ptp-reference, chapters 3 and 4.

use crate::settings::*;
use xrp_camera::slots::{prop, SlotRead, WriteOutcome};

/// What uploading a recipe to a slot will do.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// Property code and value, in the order to send them.
    pub writes: Vec<(u16, u16)>,
    /// Names of the settings that will be stored.
    pub stored: Vec<&'static str>,
    /// Names of look settings the recipe does not set; the slot keeps its own.
    pub kept: Vec<&'static str>,
    /// Things the user should know before going ahead.
    pub notes: Vec<String>,
}

/// The name of a slot setting, for messages.
pub fn setting_name(code: u16) -> &'static str {
    match code {
        prop::IMAGE_SIZE => "Image size",
        prop::IMAGE_QUALITY => "Image quality",
        prop::DYNAMIC_RANGE => "Dynamic range",
        prop::DR_PRIORITY => "D-Range Priority",
        prop::FILM_SIMULATION => "Film simulation",
        prop::MONO_WARM_COOL => "Mono colour (warm/cool)",
        prop::MONO_MAGENTA_GREEN => "Mono colour (magenta/green)",
        prop::GRAIN => "Grain",
        prop::COLOR_CHROME => "Color Chrome",
        prop::COLOR_CHROME_BLUE => "Color Chrome FX Blue",
        prop::SMOOTH_SKIN => "Smooth skin",
        prop::WHITE_BALANCE => "White balance",
        prop::WB_SHIFT_RED => "WB shift red",
        prop::WB_SHIFT_BLUE => "WB shift blue",
        prop::COLOR_TEMPERATURE => "Colour temperature",
        prop::HIGHLIGHT => "Highlight",
        prop::SHADOW => "Shadow",
        prop::COLOR => "Color",
        prop::SHARPNESS => "Sharpness",
        prop::NOISE_REDUCTION => "Noise reduction",
        prop::CLARITY => "Clarity",
        prop::LENS_OPTIMIZER => "Lens modulation optimizer",
        prop::COLOR_SPACE => "Colour space",
        _ => "Unknown setting",
    }
}

/// A signed setting as the 16-bit number the camera stores.
fn signed(value: i32) -> u16 {
    value.clamp(i16::MIN as i32, i16::MAX as i32) as i16 as u16
}

/// Works out what to send to store `settings` in a slot that now holds `current`.
pub fn plan(settings: &Settings, current: &SlotRead) -> Plan {
    let mut plan = Plan::default();
    let put = |plan: &mut Plan, code: u16, value: u16| {
        plan.writes.push((code, value));
        let name = setting_name(code);
        if code != prop::DR_PRIORITY && !plan.stored.contains(&name) {
            plan.stored.push(name);
        }
    };
    let keep = |plan: &mut Plan, name: &'static str| plan.kept.push(name);

    let film = settings.film_simulation.or(current.get(prop::FILM_SIMULATION).map(i32::from)).unwrap_or(0);
    let mono = is_monochrome(film);

    // D-Range Priority blocks dynamic range and both tones while it is on.
    let needs_priority_off =
        settings.dynamic_range.is_some() || settings.highlight.is_some() || settings.shadow.is_some();
    if needs_priority_off && current.get(prop::DR_PRIORITY).is_some_and(|v| v != 0) {
        put(&mut plan, prop::DR_PRIORITY, 0);
        plan.notes.push(
            "D-Range Priority is on in this slot. It will be switched off so the recipe's dynamic range and \
             tones can be stored."
                .into(),
        );
    }

    match settings.dynamic_range {
        Some(dr) => {
            put(&mut plan, prop::DYNAMIC_RANGE, signed(dr));
            if dr > 100 {
                plan.notes.push(format!(
                    "The camera only accepts DR{dr} if the slot's ISO is high enough (ISO is not part of a recipe)."
                ));
            }
        }
        None => keep(&mut plan, "Dynamic range"),
    }
    match settings.film_simulation {
        Some(code) => put(&mut plan, prop::FILM_SIMULATION, signed(code)),
        None => keep(&mut plan, "Film simulation"),
    }
    if mono {
        // The camera takes whole steps only: multiples of 10, within ±180.
        let step = |tenths: i32| signed(((tenths.clamp(-180, 180) as f32 / 10.0).round() as i32) * 10);
        if let Some(v) = settings.mono_warm_cool {
            put(&mut plan, prop::MONO_WARM_COOL, step(v));
        }
        if let Some(v) = settings.mono_magenta_green {
            put(&mut plan, prop::MONO_MAGENTA_GREEN, step(v));
        }
    }
    match settings.grain {
        Some(g) => {
            let value = match (g.strength, g.large) {
                (Level::Off, _) => 1,
                (Level::Weak, false) => 2,
                (Level::Strong, false) => 3,
                (Level::Weak, true) => 4,
                (Level::Strong, true) => 5,
            };
            put(&mut plan, prop::GRAIN, value);
        }
        None => keep(&mut plan, "Grain"),
    }
    match settings.color_chrome {
        Some(l) => put(&mut plan, prop::COLOR_CHROME, signed(l.code())),
        None => keep(&mut plan, "Color Chrome"),
    }
    match settings.color_chrome_blue {
        Some(l) => put(&mut plan, prop::COLOR_CHROME_BLUE, signed(l.code())),
        None => keep(&mut plan, "Color Chrome FX Blue"),
    }
    if let Some(l) = settings.smooth_skin {
        put(&mut plan, prop::SMOOTH_SKIN, signed(l.code()));
    }

    // White balance mode, then Kelvin, then the shift: this order matters.
    match settings.white_balance {
        Some(WhiteBalance::Mode(code)) => put(&mut plan, prop::WHITE_BALANCE, code as u16),
        Some(WhiteBalance::Kelvin(k)) => {
            put(&mut plan, prop::WHITE_BALANCE, WB_TEMPERATURE as u16);
            put(&mut plan, prop::COLOR_TEMPERATURE, k.clamp(2500, 10000) as u16);
        }
        Some(WhiteBalance::AsShot) | None => keep(&mut plan, "White balance"),
    }
    match (settings.wb_shift_red, settings.wb_shift_blue) {
        (None, None) => keep(&mut plan, "WB shift"),
        (red, blue) => {
            if let Some(v) = red {
                put(&mut plan, prop::WB_SHIFT_RED, signed(v.clamp(-9, 9)));
            }
            if let Some(v) = blue {
                put(&mut plan, prop::WB_SHIFT_BLUE, signed(v.clamp(-9, 9)));
            }
        }
    }

    match settings.highlight {
        Some(v) => put(&mut plan, prop::HIGHLIGHT, signed(v)),
        None => keep(&mut plan, "Highlight"),
    }
    match settings.shadow {
        Some(v) => put(&mut plan, prop::SHADOW, signed(v)),
        None => keep(&mut plan, "Shadow"),
    }
    if !mono {
        match settings.color {
            Some(v) => put(&mut plan, prop::COLOR, signed(v)),
            None => keep(&mut plan, "Color"),
        }
    }
    match settings.sharpness {
        Some(v) => put(&mut plan, prop::SHARPNESS, signed(v)),
        None => keep(&mut plan, "Sharpness"),
    }
    match settings.noise_reduction.and_then(|menu| NOISE_REDUCTION.iter().find(|(m, _)| *m == menu)) {
        Some((_, code)) => put(&mut plan, prop::NOISE_REDUCTION, *code as u16),
        None => keep(&mut plan, "Noise reduction"),
    }
    match settings.clarity {
        Some(v) => put(&mut plan, prop::CLARITY, signed(v)),
        None => keep(&mut plan, "Clarity"),
    }

    if settings.exposure_thirds.is_some_and(|t| t != 0) {
        plan.notes.push(
            "Exposure compensation is not stored in a custom slot. Set it on the camera when you shoot.".into(),
        );
    }
    plan
}

/// Whether what the slot holds after a write means what was asked for.
/// Grain "off" is written as 1 but read back as 6 or 7.
pub fn took(outcome: &WriteOutcome) -> bool {
    match (outcome.prop, outcome.wanted, outcome.read_back) {
        (_, _, None) => false,
        (prop::GRAIN, 1, Some(1 | 6 | 7)) => true,
        (_, wanted, Some(read)) => wanted == read,
    }
}

/// One line per setting that did not end up as asked, in plain words.
pub fn problems(outcomes: &[WriteOutcome]) -> Vec<String> {
    outcomes
        .iter()
        .filter(|o| !took(o))
        .map(|o| {
            let name = setting_name(o.prop);
            let why = match (o.prop, o.response) {
                (prop::DYNAMIC_RANGE, 0x201C) => {
                    " The slot's ISO may be too low for this dynamic range, or D-Range Priority is still on."
                }
                (prop::HIGHLIGHT | prop::SHADOW, 0x201C) => " D-Range Priority may still be on in this slot.",
                (prop::DR_PRIORITY, 0x201C) => {
                    " The camera does not allow changing it while the slot is at base ISO."
                }
                (prop::CLARITY, 0x201C) => " Clarity is not available while the slot saves HEIF.",
                (_, 0x200A) => " This camera has no such setting in its slots.",
                _ => "",
            };
            match o.response {
                0x2001 => format!("{name}: the camera accepted it but stored a different value.{why}"),
                code => format!("{name}: the camera refused it (code {code:#06x}).{why}"),
            }
        })
        .collect()
}

/// The look a slot holds, as recipe settings. Values the camera keeps but
/// is not using (a Kelvin under Auto white balance, toning on a colour
/// simulation) are left out, as the camera's own menu greys them out.
pub fn settings_of(slot: &SlotRead) -> Settings {
    let get = |code: u16| slot.get(code);
    let signed = |code: u16| get(code).map(|v| v as i16 as i32);
    let mut s = Settings::default();

    s.film_simulation = get(prop::FILM_SIMULATION).map(i32::from).filter(|c| film_simulation_name(*c).is_some());
    let mono = s.film_simulation.is_some_and(is_monochrome);
    let priority_on = get(prop::DR_PRIORITY).is_some_and(|v| v != 0);

    if !priority_on {
        s.dynamic_range = signed(prop::DYNAMIC_RANGE).filter(|v| [100, 200, 400].contains(v));
        s.highlight = signed(prop::HIGHLIGHT);
        s.shadow = signed(prop::SHADOW);
    }
    s.grain = get(prop::GRAIN).and_then(|v| match v {
        1 | 6 | 7 => Some(Grain { strength: Level::Off, large: v == 7 }),
        2 => Some(Grain { strength: Level::Weak, large: false }),
        3 => Some(Grain { strength: Level::Strong, large: false }),
        4 => Some(Grain { strength: Level::Weak, large: true }),
        5 => Some(Grain { strength: Level::Strong, large: true }),
        _ => None,
    });
    s.color_chrome = get(prop::COLOR_CHROME).and_then(|v| Level::from_code(v as i32));
    s.color_chrome_blue = get(prop::COLOR_CHROME_BLUE).and_then(|v| Level::from_code(v as i32));
    s.smooth_skin = get(prop::SMOOTH_SKIN).and_then(|v| Level::from_code(v as i32));

    s.white_balance = get(prop::WHITE_BALANCE).and_then(|mode| {
        let mode = mode as i32;
        if mode == WB_TEMPERATURE {
            get(prop::COLOR_TEMPERATURE).map(|k| WhiteBalance::Kelvin(k as i32))
        } else if mode != 0 {
            Some(WhiteBalance::Mode(mode))
        } else {
            None
        }
    });
    s.wb_shift_red = signed(prop::WB_SHIFT_RED);
    s.wb_shift_blue = signed(prop::WB_SHIFT_BLUE);

    if mono {
        s.mono_warm_cool = signed(prop::MONO_WARM_COOL);
        s.mono_magenta_green = signed(prop::MONO_MAGENTA_GREEN);
    } else {
        s.color = signed(prop::COLOR);
    }
    s.sharpness = signed(prop::SHARPNESS);
    s.clarity = signed(prop::CLARITY);
    s.noise_reduction =
        get(prop::NOISE_REDUCTION).and_then(|v| NOISE_REDUCTION.iter().find(|(_, c)| *c == v as i32)).map(|(m, _)| *m);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(values: &[(u16, u16)]) -> SlotRead {
        let mut all: Vec<(u16, Result<u16, u16>)> = prop::SETTINGS.map(|code| (code, Ok(0))).collect();
        for (code, value) in values {
            all.iter_mut().find(|(c, _)| c == code).unwrap().1 = Ok(*value);
        }
        SlotRead { number: 1, name: "TEST".into(), values: all }
    }

    #[test]
    fn a_colour_recipe_is_sent_in_the_order_the_camera_needs() {
        let s = Settings {
            film_simulation: Some(0x0B),
            dynamic_range: Some(200),
            grain: Some(Grain { strength: Level::Weak, large: true }),
            white_balance: Some(WhiteBalance::Kelvin(6300)),
            wb_shift_red: Some(2),
            wb_shift_blue: Some(-4),
            highlight: Some(-10),
            color: Some(20),
            noise_reduction: Some(-4),
            mono_warm_cool: Some(30),
            exposure_thirds: Some(1),
            ..Default::default()
        };
        let plan = plan(&s, &slot(&[(prop::FILM_SIMULATION, 1), (prop::DR_PRIORITY, 2)]));
        let codes: Vec<u16> = plan.writes.iter().map(|(c, _)| *c).collect();
        use prop::*;
        assert_eq!(
            codes,
            vec![
                DR_PRIORITY, DYNAMIC_RANGE, FILM_SIMULATION, GRAIN, WHITE_BALANCE, COLOR_TEMPERATURE, WB_SHIFT_RED,
                WB_SHIFT_BLUE, HIGHLIGHT, COLOR, NOISE_REDUCTION,
            ]
        );
        let value = |code: u16| plan.writes.iter().find(|(c, _)| *c == code).unwrap().1;
        assert_eq!(value(DR_PRIORITY), 0);
        assert_eq!(value(GRAIN), 4);
        assert_eq!(value(WHITE_BALANCE), 0x8007);
        assert_eq!(value(COLOR_TEMPERATURE), 6300);
        assert_eq!(value(WB_SHIFT_BLUE), (-4i16) as u16);
        assert_eq!(value(HIGHLIGHT), (-10i16) as u16);
        assert_eq!(value(NOISE_REDUCTION), 0x8000);
        // Toning is left out for a colour simulation; exposure is explained.
        assert!(!codes.contains(&MONO_WARM_COOL));
        assert!(plan.kept.contains(&"Sharpness") && plan.kept.contains(&"Shadow"));
        assert!(plan.notes.iter().any(|n| n.contains("Exposure")));
        assert!(plan.notes.iter().any(|n| n.contains("D-Range Priority")));
    }

    #[test]
    fn a_black_and_white_recipe_sends_toning_in_whole_steps_and_no_colour() {
        let s = Settings {
            film_simulation: Some(0x0C),
            color: Some(20),
            mono_warm_cool: Some(34),
            mono_magenta_green: Some(-200),
            ..Default::default()
        };
        let plan = plan(&s, &slot(&[]));
        let value = |code: u16| plan.writes.iter().find(|(c, _)| *c == code).map(|(_, v)| *v as i16);
        assert_eq!(value(prop::MONO_WARM_COOL), Some(30));
        assert_eq!(value(prop::MONO_MAGENTA_GREEN), Some(-180));
        assert_eq!(value(prop::COLOR), None);
        // The slot's own film simulation decides when the recipe names none.
        let toning_only = Settings { mono_warm_cool: Some(20), ..Default::default() };
        assert!(plan_has(&toning_only, &slot(&[(prop::FILM_SIMULATION, 0x0C)]), prop::MONO_WARM_COOL));
        assert!(!plan_has(&toning_only, &slot(&[(prop::FILM_SIMULATION, 0x01)]), prop::MONO_WARM_COOL));
    }

    fn plan_has(s: &Settings, current: &SlotRead, code: u16) -> bool {
        plan(s, current).writes.iter().any(|(c, _)| *c == code)
    }

    #[test]
    fn a_slot_reads_back_as_the_recipe_that_was_stored() {
        let s = Settings {
            film_simulation: Some(0x11),
            dynamic_range: Some(400),
            grain: Some(Grain { strength: Level::Strong, large: false }),
            color_chrome: Some(Level::Strong),
            color_chrome_blue: Some(Level::Weak),
            white_balance: Some(WhiteBalance::Mode(4)),
            wb_shift_red: Some(3),
            wb_shift_blue: Some(-5),
            highlight: Some(-15),
            shadow: Some(20),
            color: Some(-20),
            sharpness: Some(10),
            noise_reduction: Some(-2),
            clarity: Some(-30),
            ..Default::default()
        };
        let stored = slot(&plan(&s, &slot(&[])).writes);
        let mut back = settings_of(&stored);
        // The fake slot holds 0 for what the recipe did not set.
        back.smooth_skin = None;
        assert_eq!(back, s);
    }

    #[test]
    fn unused_values_in_a_slot_are_not_shown_as_settings() {
        let read = slot(&[
            (prop::FILM_SIMULATION, 0x01),
            (prop::WHITE_BALANCE, 2),
            (prop::COLOR_TEMPERATURE, 10000),
            (prop::MONO_WARM_COOL, 180),
            (prop::DR_PRIORITY, 0x8000),
            (prop::DYNAMIC_RANGE, 0xFFFF),
            (prop::GRAIN, 7),
        ]);
        let s = settings_of(&read);
        assert_eq!(s.white_balance, Some(WhiteBalance::Mode(2)));
        assert_eq!(s.mono_warm_cool, None);
        assert_eq!((s.dynamic_range, s.highlight), (None, None));
        assert_eq!(s.grain, Some(Grain { strength: Level::Off, large: true }));
    }

    #[test]
    fn grain_off_counts_as_stored_and_refusals_are_explained() {
        let ok = WriteOutcome { prop: prop::GRAIN, wanted: 1, response: 0x2001, read_back: Some(7) };
        let refused = WriteOutcome { prop: prop::DYNAMIC_RANGE, wanted: 400, response: 0x201C, read_back: Some(100) };
        let changed = WriteOutcome { prop: prop::SHARPNESS, wanted: 10, response: 0x2001, read_back: Some(0) };
        assert!(took(&ok) && !took(&refused) && !took(&changed));
        let lines = problems(&[ok, refused, changed]);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Dynamic range") && lines[0].contains("ISO"));
        assert!(lines[1].contains("Sharpness") && lines[1].contains("different value"));
    }
}
