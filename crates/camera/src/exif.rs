//! Reading the details a camera writes into a photo (EXIF), including
//! Fujifilm's own block of settings (the "MakerNote").
//!
//! This is a small reader for exactly what X Recipe Bench shows and uses. It
//! reads the first EXIF block of a JPEG, or of the preview JPEG inside a RAF.
//! Layout facts follow the EXIF standard and the public notes in
//! fujifilm-ptp-reference (CC BY 4.0).

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// How much of a file is read to find the EXIF block. It sits at the start.
const HEAD: u64 = 256 * 1024;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Exif {
    pub make: String,
    pub model: String,
    pub lens: String,
    /// EXIF orientation, 1..=8. 1 means "as stored".
    pub orientation: u16,
    /// Shutter time as a fraction of a second: (1, 250) is 1/250 s.
    pub exposure_time: Option<(u32, u32)>,
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

/// A TIFF structure: byte order plus the bytes offsets are relative to.
struct Tiff<'a> {
    data: &'a [u8],
    little: bool,
}

struct Entry {
    tag: u16,
    kind: u16,
    count: usize,
    /// Where the values are, within `data`.
    at: usize,
}

impl<'a> Tiff<'a> {
    fn u16(&self, at: usize) -> Option<u16> {
        let b: [u8; 2] = self.data.get(at..at + 2)?.try_into().ok()?;
        Some(if self.little { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    }
    fn u32(&self, at: usize) -> Option<u32> {
        let b: [u8; 4] = self.data.get(at..at + 4)?.try_into().ok()?;
        Some(if self.little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    }

    fn entries(&self, ifd: usize) -> Vec<Entry> {
        let mut out = Vec::new();
        let Some(count) = self.u16(ifd) else { return out };
        for i in 0..(count as usize).min(4096) {
            let at = ifd + 2 + 12 * i;
            let (Some(tag), Some(kind), Some(n)) = (self.u16(at), self.u16(at + 2), self.u32(at + 4)) else {
                break;
            };
            let size: usize = match kind {
                1 | 2 | 6 | 7 => 1,
                3 | 8 => 2,
                4 | 9 => 4,
                5 | 10 => 8,
                _ => continue,
            };
            let n = n as usize;
            let Some(total) = size.checked_mul(n) else { continue };
            let value_at = if total <= 4 {
                at + 8
            } else {
                match self.u32(at + 8) {
                    Some(offset) => offset as usize,
                    None => continue,
                }
            };
            if value_at.checked_add(total).map(|end| end <= self.data.len()).unwrap_or(false) {
                out.push(Entry { tag, kind, count: n, at: value_at });
            }
        }
        out
    }

    /// The entry's values as integers (rationals are skipped).
    fn integers(&self, e: &Entry) -> Vec<i64> {
        (0..e.count.min(64))
            .filter_map(|i| match e.kind {
                1 | 7 => self.data.get(e.at + i).map(|b| *b as i64),
                6 => self.data.get(e.at + i).map(|b| *b as i8 as i64),
                3 => self.u16(e.at + 2 * i).map(|v| v as i64),
                8 => self.u16(e.at + 2 * i).map(|v| v as i16 as i64),
                4 => self.u32(e.at + 4 * i).map(|v| v as i64),
                9 => self.u32(e.at + 4 * i).map(|v| v as i32 as i64),
                _ => None,
            })
            .collect()
    }

    fn text(&self, e: &Entry) -> String {
        let bytes = &self.data[e.at..e.at + e.count];
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).trim().to_string()
    }

    /// A rational as (numerator, denominator); signed for type 10.
    fn rational(&self, e: &Entry) -> Option<(i64, i64)> {
        let (n, d) = (self.u32(e.at)?, self.u32(e.at + 4)?);
        Some(if e.kind == 10 { (n as i32 as i64, d as i32 as i64) } else { (n as i64, d as i64) })
    }

    fn number(&self, e: &Entry) -> Option<f64> {
        match e.kind {
            5 | 10 => self.rational(e).filter(|(_, d)| *d != 0).map(|(n, d)| n as f64 / d as f64),
            _ => self.integers(e).first().map(|v| *v as f64),
        }
    }
}

/// Finds the EXIF block of a JPEG: (offset of the TIFF data, its length).
fn find_tiff(jpeg: &[u8]) -> Option<(usize, usize)> {
    if !jpeg.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut at = 2;
    while at + 4 <= jpeg.len() {
        if jpeg[at] != 0xFF {
            return None;
        }
        let marker = jpeg[at + 1];
        if marker == 0xDA || marker == 0xD9 {
            return None; // image data starts; no EXIF before it
        }
        let len = u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]) as usize;
        if len < 2 {
            return None;
        }
        let body = at + 4;
        let end = (at + 2 + len).min(jpeg.len());
        if marker == 0xE1 && jpeg.get(body..body + 6) == Some(b"Exif\0\0") {
            return Some((body + 6, end.saturating_sub(body + 6)));
        }
        at += 2 + len;
    }
    None
}

fn open_tiff(data: &[u8]) -> Option<(Tiff<'_>, usize)> {
    let little = match data.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let tiff = Tiff { data, little };
    if tiff.u16(2)? != 42 {
        return None;
    }
    let ifd0 = tiff.u32(4)? as usize;
    Some((tiff, ifd0))
}

/// Reads the EXIF block from the start of a JPEG. `None` if there is none.
pub fn parse_jpeg(jpeg: &[u8]) -> Option<Exif> {
    let (start, len) = find_tiff(jpeg)?;
    let (tiff, ifd0) = open_tiff(&jpeg[start..start + len])?;
    let mut exif = Exif { orientation: 1, ..Default::default() };
    let mut exif_ifd = None;

    for e in tiff.entries(ifd0) {
        match e.tag {
            0x010F => exif.make = tiff.text(&e),
            0x0110 => exif.model = tiff.text(&e),
            0x0112 => {
                let v = tiff.integers(&e).first().copied().unwrap_or(1);
                exif.orientation = if (1..=8).contains(&v) { v as u16 } else { 1 };
            }
            0x8769 => exif_ifd = tiff.integers(&e).first().map(|v| *v as usize),
            _ => {}
        }
    }
    for e in exif_ifd.map(|ifd| tiff.entries(ifd)).unwrap_or_default() {
        match e.tag {
            0x829A => {
                exif.exposure_time =
                    tiff.rational(&e).filter(|(n, d)| *n > 0 && *d > 0).map(|(n, d)| (n as u32, d as u32))
            }
            0x829D => exif.f_number = tiff.number(&e),
            0x8827 => exif.iso = tiff.integers(&e).first().map(|v| *v as u32),
            0x9003 => exif.date_time = Some(tiff.text(&e)).filter(|t| !t.is_empty()),
            0x9204 => exif.exposure_bias = tiff.number(&e),
            0x920A => exif.focal_length = tiff.number(&e),
            0xA434 => exif.lens = tiff.text(&e),
            0x927C => exif.fuji = fuji_maker_note(&tiff.data[e.at..e.at + e.count]),
            _ => {}
        }
    }
    Some(exif)
}

/// Fujifilm's block: "FUJIFILM", a little-endian offset, then a directory
/// whose offsets are relative to the start of the block.
fn fuji_maker_note(note: &[u8]) -> HashMap<u16, Vec<i64>> {
    let mut tags = HashMap::new();
    if !note.starts_with(b"FUJIFILM") {
        return tags;
    }
    let tiff = Tiff { data: note, little: true };
    let Some(ifd) = tiff.u32(8) else { return tags };
    for e in tiff.entries(ifd as usize) {
        let values = tiff.integers(&e);
        if !values.is_empty() {
            tags.entry(e.tag).or_insert(values);
        }
    }
    tags
}

/// Reads the details of a photo file: a JPEG, or a RAF (via its preview JPEG).
pub fn read_file(path: &Path) -> std::io::Result<Option<Exif>> {
    let mut file = std::fs::File::open(path)?;
    let mut start = [0u8; 2];
    if file.read_exact(&mut start).is_err() {
        return Ok(None);
    }
    let offset = if start == [0xFF, 0xD8] {
        0
    } else {
        match crate::raf::read_header(path)? {
            Some(h) if h.jpeg_len > 0 => h.jpeg_offset,
            _ => return Ok(None),
        }
    };
    file.seek(SeekFrom::Start(offset))?;
    let mut head = Vec::new();
    file.take(HEAD).read_to_end(&mut head)?;
    Ok(parse_jpeg(&head))
}

/// Puts `original`'s EXIF block into `reencoded`, with the orientation set
/// to "as stored" because the pixels have already been turned upright.
/// Returns `reencoded` unchanged if the original has no EXIF block.
pub fn carry_over_upright(original: &[u8], reencoded: &[u8]) -> Vec<u8> {
    let Some((start, len)) = find_tiff(original) else { return reencoded.to_vec() };
    if !reencoded.starts_with(&[0xFF, 0xD8]) || len + 8 > 0xFFFF {
        return reencoded.to_vec();
    }
    let mut tiff_bytes = original[start..start + len].to_vec();
    // Find the orientation entry in the first directory and set it to 1.
    let orientation_at = open_tiff(&tiff_bytes).and_then(|(tiff, ifd0)| {
        tiff.entries(ifd0).into_iter().find(|e| e.tag == 0x0112 && e.kind == 3).map(|e| (e.at, tiff.little))
    });
    if let Some((at, little)) = orientation_at {
        let one = if little { 1u16.to_le_bytes() } else { 1u16.to_be_bytes() };
        tiff_bytes[at..at + 2].copy_from_slice(&one);
    }

    let mut out = Vec::with_capacity(reencoded.len() + tiff_bytes.len() + 10);
    out.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE1]);
    out.extend_from_slice(&((tiff_bytes.len() + 8) as u16).to_be_bytes());
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(&tiff_bytes);
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
        assert_eq!(exif.exposure_time, Some((1, 250)));
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
