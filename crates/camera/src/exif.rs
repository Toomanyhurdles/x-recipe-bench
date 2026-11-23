//! Reading the details a camera writes into a photo (EXIF), including
//! Fujifilm's own block of settings (the "MakerNote").
//!
//! The reading itself is done by craft-libs: `craft-metadata` finds the EXIF
//! block in a JPEG or RAF and reads the standard fields, and `craft-tiff`
//! parses the Fujifilm MakerNote. This module only turns their results into
//! the small [`Exif`] record the rest of X Recipe Bench uses, and keeps the
//! Fujifilm values as plain numbers so recipes can be rebuilt from them.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use craft_tiff::makernote::{parse_makernote, MakerNoteKind};
use craft_tiff::{Tiff, Value};

/// How much of a file is read to find the EXIF block of a JPEG. It sits at the start.
const HEAD: u64 = 256 * 1024;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Exif {
    pub make: String,
    pub model: String,
    pub lens: String,
    /// EXIF orientation, 1..=8. 1 means "as stored".
    pub orientation: u16,
    /// Shutter time in seconds: 0.004 is 1/250 s.
    pub exposure_time: Option<f64>,
    pub f_number: Option<f64>,
    pub iso: Option<u32>,
    /// Exposure compensation in EV.
    pub exposure_bias: Option<f64>,
    /// Focal length in millimetres.
    pub focal_length: Option<f64>,
    /// When the photo was taken, as the camera wrote it: "2026:11:17 09:12:44".
    pub date_time: Option<String>,
    /// Fujifilm's settings block: tag → values. Empty for other makes.
    pub fuji: HashMap<u16, Vec<i64>>,
}

/// A shutter time as people write it: "1/250 s", "2.5 s".
pub fn shutter_label(seconds: f64) -> String {
    craft_metadata::format_exposure_time(seconds).map(|t| format!("{t} s")).unwrap_or_default()
}

/// Reads the EXIF block from the start of a JPEG. `None` if there is none.
pub fn parse_jpeg(jpeg: &[u8]) -> Option<Exif> {
    if !jpeg.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    from_block(&craft_metadata::jpeg_segments(jpeg).exif?)
}

/// Reads a TIFF-structured EXIF block (as found inside a JPEG's APP1 segment).
fn from_block(block: &[u8]) -> Option<Exif> {
    let data = craft_metadata::strip_exif_header(block);
    // A block that is not a readable TIFF structure counts as no EXIF at all.
    let tiff = Tiff::parse(data).ok()?;
    let ifd0 = tiff.ifds.first()?;
    let m = craft_metadata::read_exif(data);

    let date_time = ifd0.exif.as_ref().and_then(|e| e.string(0x9003)).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let fuji = ifd0
        .exif
        .as_ref()
        .and_then(|e| e.get(0x927C))
        .and_then(|note| parse_makernote(data, note.offset, note.count() as u64, tiff.order, m.make.as_deref().unwrap_or("")))
        .filter(|note| note.kind == MakerNoteKind::Fujifilm)
        .map(|note| note.ifd.entries.iter().filter_map(|e| Some((e.tag, integers(&e.value)?))).collect())
        .unwrap_or_default();

    Some(Exif {
        make: m.make.unwrap_or_default().trim().to_string(),
        model: m.model.unwrap_or_default().trim().to_string(),
        lens: m.lens_model.unwrap_or_default().trim().to_string(),
        orientation: m.orientation.map(|o| o.to_exif()).unwrap_or(1),
        exposure_time: m.exposure_time.filter(|t| *t > 0.0),
        f_number: m.f_number,
        iso: m.iso,
        exposure_bias: m.exposure_bias,
        focal_length: m.focal_length,
        date_time,
        fuji,
    })
}

/// A MakerNote value as whole numbers (at most 64 of them). Text, fractions and
/// empty values give `None`: no recipe setting is stored that way.
fn integers(value: &Value) -> Option<Vec<i64>> {
    if matches!(value, Value::Ascii(_) | Value::Rational(_) | Value::SRational(_) | Value::Float(_) | Value::Double(_)) {
        return None;
    }
    let n = value.count().min(64);
    let values: Vec<i64> = (0..n).filter_map(|i| value.get_i64(i)).collect();
    (!values.is_empty()).then_some(values)
}

/// Reads the details of a photo file: a JPEG, or a RAF (via its preview JPEG).
pub fn read_file(path: &Path) -> std::io::Result<Option<Exif>> {
    let mut start = Vec::new();
    std::fs::File::open(path)?.take(HEAD).read_to_end(&mut start)?;
    if start.starts_with(&[0xFF, 0xD8]) {
        return Ok(parse_jpeg(&start));
    }
    if crate::raf::is_raf(&start) {
        return Ok(crate::raf::read_embedded_jpeg(path)?.and_then(|jpeg| parse_jpeg(&jpeg)));
    }
    Ok(None)
}

/// `original`'s EXIF block, with the orientation set to "as stored" because the
/// pixels have already been turned upright, ready to go into a new JPEG as an
/// APP1 segment: `(0xE1, "Exif\0\0" + block)`. `None` if it has no EXIF block.
pub fn upright_segment(original: &[u8]) -> Option<(u8, Vec<u8>)> {
    let block = craft_metadata::jpeg_segments(original).exif?;
    let upright = craft_metadata::edit::upright_exif(&block);
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(&upright);
    // One JPEG segment holds at most 65533 bytes of payload.
    (payload.len() <= 0xFFFF - 2).then_some((0xE1, payload))
}

/// Puts `original`'s EXIF block into `reencoded`, with the orientation set
/// to "as stored" because the pixels have already been turned upright.
/// Returns `reencoded` unchanged if the original has no EXIF block.
pub fn carry_over_upright(original: &[u8], reencoded: &[u8]) -> Vec<u8> {
    let Some((marker, payload)) = upright_segment(original) else { return reencoded.to_vec() };
    if !reencoded.starts_with(&[0xFF, 0xD8]) {
        return reencoded.to_vec();
    }
    let mut out = Vec::with_capacity(reencoded.len() + payload.len() + 4);
    out.extend_from_slice(&[0xFF, 0xD8, 0xFF, marker]);
    out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(&reencoded[2..]);
    out
}

/// Builds a small JPEG with an EXIF block, for tests here and in other crates.
#[doc(hidden)]
pub fn test_jpeg(orientation: u16, fuji: &[(u16, u16, Vec<i64>)]) -> Vec<u8> {
    // TIFF, little-endian. Layout: header, IFD0, strings, Exif IFD, values, MakerNote.
    let mut t: Vec<u8> = Vec::new();
    t.extend_from_slice(b"II");
    t.extend_from_slice(&42u16.to_le_bytes());
    t.extend_from_slice(&8u32.to_le_bytes());
    let entry = |t: &mut Vec<u8>, tag: u16, kind: u16, count: u32, value: u32| {
        t.extend_from_slice(&tag.to_le_bytes());
        t.extend_from_slice(&kind.to_le_bytes());
        t.extend_from_slice(&count.to_le_bytes());
        t.extend_from_slice(&value.to_le_bytes());
    };
    // IFD0: 4 entries → 2 + 48 + 4 = 54 bytes, ends at 62.
    let (make_at, model_at, exif_ifd_at) = (62u32, 71u32, 76u32);
    t.extend_from_slice(&4u16.to_le_bytes());
    entry(&mut t, 0x010F, 2, 9, make_at);
    entry(&mut t, 0x0110, 2, 5, model_at);
    entry(&mut t, 0x0112, 3, 1, orientation as u32);
    entry(&mut t, 0x8769, 4, 1, exif_ifd_at);
    t.extend_from_slice(&0u32.to_le_bytes());
    t.extend_from_slice(b"FUJIFILM\0");
    t.extend_from_slice(b"X-T4\0");
    assert_eq!(t.len() as u32, exif_ifd_at);

    // MakerNote bytes.
    let mut note: Vec<u8> = Vec::new();
    note.extend_from_slice(b"FUJIFILM");
    note.extend_from_slice(&12u32.to_le_bytes());
    note.extend_from_slice(&(fuji.len() as u16).to_le_bytes());
    let mut extra: Vec<u8> = Vec::new();
    let extra_base = 12 + 2 + 12 * fuji.len() as u32 + 4;
    for (tag, kind, values) in fuji {
        note.extend_from_slice(&tag.to_le_bytes());
        note.extend_from_slice(&kind.to_le_bytes());
        note.extend_from_slice(&(values.len() as u32).to_le_bytes());
        let mut bytes: Vec<u8> = Vec::new();
        for v in values {
            match kind {
                3 | 8 => bytes.extend_from_slice(&(*v as u16).to_le_bytes()),
                _ => bytes.extend_from_slice(&(*v as u32).to_le_bytes()),
            }
        }
        if bytes.len() <= 4 {
            bytes.resize(4, 0);
            note.extend_from_slice(&bytes);
        } else {
            note.extend_from_slice(&(extra_base + extra.len() as u32).to_le_bytes());
            extra.extend_from_slice(&bytes);
        }
    }
    note.extend_from_slice(&0u32.to_le_bytes());
    note.extend_from_slice(&extra);

    // Exif IFD: 7 entries → 2 + 84 + 4 = 90 bytes.
    let values_at = exif_ifd_at + 90;
    let date = b"2026:11:17 09:12:44\0";
    let lens = b"XF23mmF2 R WR\0";
    let (time_at, f_at, bias_at, focal_at) = (values_at, values_at + 8, values_at + 16, values_at + 24);
    let date_at = values_at + 32;
    let lens_at = date_at + date.len() as u32;
    let note_at = lens_at + lens.len() as u32;
    t.extend_from_slice(&8u16.to_le_bytes());
    entry(&mut t, 0x829A, 5, 1, time_at);
    entry(&mut t, 0x829D, 5, 1, f_at);
    entry(&mut t, 0x8827, 3, 1, 640);
    entry(&mut t, 0x9003, 2, date.len() as u32, date_at);
    entry(&mut t, 0x9204, 10, 1, bias_at);
    entry(&mut t, 0x920A, 5, 1, focal_at);
    entry(&mut t, 0xA434, 2, lens.len() as u32, lens_at);
    entry(&mut t, 0x927C, 7, note.len() as u32, note_at + 12);
    t.extend_from_slice(&0u32.to_le_bytes());
    // The eighth entry made the directory 12 bytes longer than planned above.
    let shift = 12u32;
    let fix = |t: &mut Vec<u8>, entry_index: usize| {
        let at = exif_ifd_at as usize + 2 + 12 * entry_index + 8;
        let v = u32::from_le_bytes(t[at..at + 4].try_into().unwrap()) + shift;
        t[at..at + 4].copy_from_slice(&v.to_le_bytes());
    };
    for i in [0usize, 1, 3, 4, 5, 6] {
        fix(&mut t, i);
    }
    for pair in [(1u32, 250u32), (28, 10)] {
        t.extend_from_slice(&pair.0.to_le_bytes());
        t.extend_from_slice(&pair.1.to_le_bytes());
    }
    t.extend_from_slice(&(-2i32).to_le_bytes());
    t.extend_from_slice(&3i32.to_le_bytes());
    t.extend_from_slice(&23u32.to_le_bytes());
    t.extend_from_slice(&1u32.to_le_bytes());
    t.extend_from_slice(date);
    t.extend_from_slice(lens);
    assert_eq!(t.len() as u32, note_at + shift);
    t.extend_from_slice(&note);

    let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
    jpeg.extend_from_slice(&((t.len() + 8) as u16).to_be_bytes());
    jpeg.extend_from_slice(b"Exif\0\0");
    jpeg.extend_from_slice(&t);
    jpeg.extend_from_slice(&[0xFF, 0xD9]);
    jpeg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_shooting_details() {
        let exif = parse_jpeg(&test_jpeg(6, &[])).unwrap();
        assert_eq!((exif.make.as_str(), exif.model.as_str()), ("FUJIFILM", "X-T4"));
        assert_eq!(exif.lens, "XF23mmF2 R WR");
        assert_eq!(exif.orientation, 6);
        assert!((exif.exposure_time.unwrap() - 0.004).abs() < 1e-9);
        assert_eq!(shutter_label(exif.exposure_time.unwrap()), "1/250 s");
        assert_eq!(exif.f_number, Some(2.8));
        assert_eq!(exif.iso, Some(640));
        assert!((exif.exposure_bias.unwrap() + 0.6667).abs() < 0.001);
        assert_eq!(exif.focal_length, Some(23.0));
        assert_eq!(exif.date_time.as_deref(), Some("2026:11:17 09:12:44"));
    }

    #[test]
    fn reads_the_fujifilm_block_including_signed_and_paired_values() {
        let jpeg = test_jpeg(1, &[(0x1401, 3, vec![0x600]), (0x1041, 9, vec![-24]), (0x100A, 9, vec![60, -40])]);
        let exif = parse_jpeg(&jpeg).unwrap();
        assert_eq!(exif.fuji.get(&0x1401), Some(&vec![0x600]));
        assert_eq!(exif.fuji.get(&0x1041), Some(&vec![-24]));
        assert_eq!(exif.fuji.get(&0x100A), Some(&vec![60, -40]));
    }

    #[test]
    fn ignores_things_that_are_not_photos() {
        assert!(parse_jpeg(b"hello").is_none());
        assert!(parse_jpeg(&[0xFF, 0xD8, 0xFF, 0xD9]).is_none());
        // Cut-off data must not panic.
        let jpeg = test_jpeg(1, &[(0x1401, 3, vec![0x600])]);
        for cut in [10, 40, 90, 150, jpeg.len() - 20] {
            let _ = parse_jpeg(&jpeg[..cut]);
        }
    }

    #[test]
    fn carries_details_into_a_reencoded_photo_and_marks_it_upright() {
        let original = test_jpeg(6, &[(0x1401, 3, vec![0x600])]);
        let reencoded = [0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x02, 0xFF, 0xD9];
        let out = carry_over_upright(&original, &reencoded);
        let exif = parse_jpeg(&out).unwrap();
        assert_eq!(exif.orientation, 1);
        assert_eq!(exif.model, "X-T4");
        assert_eq!(exif.fuji.get(&0x1401), Some(&vec![0x600]));
        assert!(out.ends_with(&reencoded[2..]));
        // No EXIF in the original: the re-encoded photo is returned as is.
        assert_eq!(carry_over_upright(&[0xFF, 0xD8, 0xFF, 0xD9], &reencoded), reencoded);
    }
}
