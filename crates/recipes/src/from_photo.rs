//! Working out the look a photo was shot with, from the settings block a
//! Fujifilm camera writes into every JPEG and RAF.
//!
//! The photo uses different numbers from recipe files and from the camera's
//! conversion profile, so everything is translated here. Value tables follow
//! the published ExifTool tables as summarised in fujifilm-ptp-reference.

use crate::settings::*;
use xrp_camera::exif::Exif;

mod tag {
    pub const SHARPNESS: u16 = 0x1001;
    pub const WHITE_BALANCE: u16 = 0x1002;
    pub const SATURATION: u16 = 0x1003;
    pub const COLOR_TEMPERATURE: u16 = 0x1005;
    pub const WB_FINE_TUNE: u16 = 0x100A;
    pub const NOISE_REDUCTION: u16 = 0x100E;
    pub const CLARITY: u16 = 0x100F;
    pub const SHADOW: u16 = 0x1040;
    pub const HIGHLIGHT: u16 = 0x1041;
    pub const GRAIN_ROUGHNESS: u16 = 0x1047;
    pub const COLOR_CHROME: u16 = 0x1048;
    pub const MONO_WARM_COOL: u16 = 0x1049;
    pub const MONO_MAGENTA_GREEN: u16 = 0x104B;
    pub const GRAIN_SIZE: u16 = 0x104C;
    pub const COLOR_CHROME_BLUE: u16 = 0x104E;
    pub const FILM_MODE: u16 = 0x1401;
    pub const DR_SETTING: u16 = 0x1402;
    pub const DR_DEVELOPMENT: u16 = 0x1403;
    pub const DR_AUTO: u16 = 0x140B;
    pub const DR_PRIORITY: u16 = 0x1443;
}

fn lookup(table: &[(i64, i32)], value: i64) -> Option<i32> {
    table.iter().find(|(raw, _)| *raw == value).map(|(_, v)| *v)
}

/// The look the photo was shot with, or `None` if it carries no Fujifilm settings.
pub fn look(exif: &Exif) -> Option<Settings> {
    if exif.fuji.is_empty() {
        return None;
    }
    let one = |tag: u16| exif.fuji.get(&tag).and_then(|v| v.first()).copied();
    let mut s = Settings::default();

    // Black-and-white simulations are recorded in the Saturation tag.
    let saturation = one(tag::SATURATION);
    let mono = saturation.and_then(|v| {
        lookup(
            &[
                (0x300, 0x06),
                (0x301, 0x08),
                (0x302, 0x07),
                (0x303, 0x09),
                (0x310, 0x0A),
                (0x500, 0x0C),
                (0x501, 0x0E),
                (0x502, 0x0D),
                (0x503, 0x0F),
            ],
            v,
        )
    });
    s.film_simulation = mono.or_else(|| {
        one(tag::FILM_MODE).and_then(|v| {
            lookup(
                &[
                    (0x000, 0x01),
                    (0x120, 0x03),
                    (0x200, 0x02),
                    (0x400, 0x02),
                    (0x500, 0x05),
                    (0x501, 0x04),
                    (0x600, 0x0B),
                    (0x700, 0x10),
                    (0x800, 0x11),
                    (0x900, 0x12),
                    (0xA00, 0x13),
                    (0xB00, 0x14),
                ],
                v,
            )
        })
    });
    if mono.is_none() {
        s.color = saturation
            .and_then(|v| {
                lookup(
                    &[(0, 0), (0x80, 1), (0x100, 2), (0xC0, 3), (0xE0, 4), (0x180, -1), (0x400, -2), (0x4C0, -3), (0x4E0, -4)],
                    v,
                )
            })
            .map(|v| v * 10);
    }

    s.sharpness = one(tag::SHARPNESS)
        .and_then(|v| lookup(&[(0, -4), (1, -3), (2, -2), (0x82, -1), (3, 0), (0x84, 1), (4, 2), (5, 3), (6, 4)], v))
        .map(|v| v * 10);
    s.noise_reduction = one(tag::NOISE_REDUCTION).and_then(|v| {
        lookup(
            &[(0, 0), (0x180, 1), (0x100, 2), (0x1C0, 3), (0x1E0, 4), (0x280, -1), (0x200, -2), (0x2C0, -3), (0x2E0, -4)],
            v,
        )
    });

    // Tones are stored as −value × 16: −24 is +1.5.
    let tone = |tag: u16| {
        one(tag).filter(|raw| raw % 8 == 0 && (-64..=32).contains(raw)).map(|raw| (-raw * 10 / 16) as i32)
    };
    s.highlight = tone(tag::HIGHLIGHT);
    s.shadow = tone(tag::SHADOW);
    // Clarity is stored × 1000.
    s.clarity = one(tag::CLARITY).filter(|raw| raw % 1000 == 0 && raw.abs() <= 5000).map(|raw| (raw / 100) as i32);

    let level = |tag: u16| one(tag).and_then(|v| lookup(&[(0, 0), (32, 1), (64, 2)], v)).map(|i| match i {
        0 => Level::Off,
        1 => Level::Weak,
        _ => Level::Strong,
    });
    s.color_chrome = level(tag::COLOR_CHROME);
    s.color_chrome_blue = level(tag::COLOR_CHROME_BLUE);
    s.grain = level(tag::GRAIN_ROUGHNESS).and_then(|strength| match (strength, one(tag::GRAIN_SIZE)) {
        (Level::Off, _) => Some(Grain { strength, large: false }),
        (_, Some(16)) => Some(Grain { strength, large: false }),
        (_, Some(32)) => Some(Grain { strength, large: true }),
        _ => None,
    });

    // Monochromatic colour: one signed byte each, whatever type is declared.
    if s.film_simulation.map(is_monochrome).unwrap_or(false) {
        let signed = |tag: u16| one(tag).map(|v| (v as u8 as i8) as i32).filter(|v| (-18..=18).contains(v));
        s.mono_warm_cool = signed(tag::MONO_WARM_COOL).map(|v| v * 10);
        s.mono_magenta_green = signed(tag::MONO_MAGENTA_GREEN).map(|v| v * 10);
    }

    s.white_balance = one(tag::WHITE_BALANCE).and_then(|v| match v {
        0xFF0 => one(tag::COLOR_TEMPERATURE)
            .filter(|k| (2000..=12000).contains(k))
            .map(|k| WhiteBalance::Kelvin(k as i32)),
        v => lookup(
            &[
                (0x000, 0x0002),
                (0x001, 0x8020),
                (0x002, 0x8021),
                (0x100, 0x0004),
                (0x200, 0x8006),
                (0x300, 0x8001),
                (0x301, 0x8002),
                (0x302, 0x8003),
                (0x400, 0x0006),
                (0x600, 0x0008),
                (0xF00, 0x8008),
                (0xF01, 0x8009),
                (0xF02, 0x800A),
            ],
            v,
        )
        .map(WhiteBalance::Mode),
    });
    // The shift is stored either directly or × 20, depending on the camera.
    if let Some(pair) = exif.fuji.get(&tag::WB_FINE_TUNE).filter(|v| v.len() == 2) {
        let scaled: Option<Vec<i32>> = if pair.iter().all(|v| v % 20 == 0) {
            Some(pair.iter().map(|v| (v / 20) as i32).collect())
        } else {
            Some(pair.iter().map(|v| *v as i32).collect())
        };
        if let Some(shift) = scaled.filter(|s| s.iter().all(|v| (-9..=9).contains(v))) {
            s.wb_shift_red = Some(shift[0]);
            s.wb_shift_blue = Some(shift[1]);
        }
    }

    // With D-Range Priority on, dynamic range is not an independent setting.
    if one(tag::DR_PRIORITY).is_none() {
        let percent = match one(tag::DR_SETTING) {
            Some(1) => one(tag::DR_DEVELOPMENT),
            Some(0) => one(tag::DR_AUTO),
            _ => None,
        };
        s.dynamic_range = percent.filter(|p| [100, 200, 400].contains(p)).map(|p| p as i32);
    }

    Some(s).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use xrp_camera::exif::{parse_jpeg, test_jpeg};

    fn look_of(tags: &[(u16, u16, Vec<i64>)]) -> Option<Settings> {
        look(&parse_jpeg(&test_jpeg(1, tags)).unwrap())
    }

    #[test]
    fn reads_a_colour_look() {
        let s = look_of(&[
            (0x1401, 3, vec![0x600]),      // Classic Chrome
            (0x1003, 3, vec![0x100]),      // Color +2
            (0x1001, 3, vec![0x82]),       // Sharpness −1
            (0x100E, 3, vec![0x2E0]),      // NR −4
            (0x1041, 9, vec![16]),         // Highlight −1
            (0x1040, 9, vec![-24]),        // Shadow +1.5
            (0x100F, 9, vec![-2000]),      // Clarity −2
            (0x1047, 9, vec![64]),         // Grain strong
            (0x104C, 3, vec![32]),         // ...large
            (0x1048, 9, vec![32]),         // Color Chrome weak
            (0x104E, 9, vec![0]),          // FX Blue off
            (0x1002, 3, vec![0x100]),      // Daylight
            (0x100A, 9, vec![60, -100]),   // R+3 B−5, stored × 20
            (0x1402, 3, vec![1]),
            (0x1403, 3, vec![200]),
        ])
        .unwrap();
        assert_eq!(s.film_simulation, Some(0x0B));
        assert_eq!((s.color, s.sharpness, s.noise_reduction), (Some(20), Some(-10), Some(-4)));
        assert_eq!((s.highlight, s.shadow, s.clarity), (Some(-10), Some(15), Some(-20)));
        assert_eq!(s.grain, Some(Grain { strength: Level::Strong, large: true }));
        assert_eq!((s.color_chrome, s.color_chrome_blue), (Some(Level::Weak), Some(Level::Off)));
        assert_eq!(s.white_balance, Some(WhiteBalance::Mode(0x0004)));
        assert_eq!((s.wb_shift_red, s.wb_shift_blue), (Some(3), Some(-5)));
        assert_eq!(s.dynamic_range, Some(200));
    }

    #[test]
    fn reads_a_black_and_white_look_with_toning_and_kelvin() {
        let s = look_of(&[
            (0x1401, 3, vec![0x000]),
            (0x1003, 3, vec![0x501]), // Acros + red filter
            (0x1049, 9, vec![0xEE]),  // warm/cool −18, as one signed byte
            (0x104B, 9, vec![5]),
            (0x1002, 3, vec![0xFF0]),
            (0x1005, 3, vec![5900]),
            (0x100A, 9, vec![2, -4]), // stored directly
        ])
        .unwrap();
        assert_eq!(s.film_simulation, Some(0x0E));
        assert_eq!(s.color, None, "saturation means the simulation here, not a colour value");
        assert_eq!((s.mono_warm_cool, s.mono_magenta_green), (Some(-180), Some(50)));
        assert_eq!(s.white_balance, Some(WhiteBalance::Kelvin(5900)));
        assert_eq!((s.wb_shift_red, s.wb_shift_blue), (Some(2), Some(-4)));
    }

    #[test]
    fn leaves_out_what_it_cannot_be_sure_of() {
        assert_eq!(look_of(&[]), None);
        let s = look_of(&[
            (0x1401, 3, vec![0x800]),
            (0x100F, 9, vec![1500]),      // not a whole clarity step
            (0x100A, 9, vec![21, 3]),     // out of range either way
            (0x1047, 9, vec![32]),        // grain on, size missing
            (0x1402, 3, vec![1]),
            (0x1403, 3, vec![800]),       // not a recipe dynamic range
        ])
        .unwrap();
        assert_eq!(s.film_simulation, Some(0x11));
        assert_eq!((s.clarity, s.wb_shift_red, s.grain, s.dynamic_range), (None, None, None, None));
    }
}
