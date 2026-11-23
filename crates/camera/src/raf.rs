//! Reading facts from a RAF file on disk. No camera needed.
//!
//! The file format work is done by `craft-raw` from craft-libs. A RAF keeps a
//! full-size preview JPEG near its start (the as-shot look); it is read
//! without loading the rest of the file, which is mostly sensor data.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// How much of a RAF is read at first to reach its preview. An X-T4 RAF keeps
/// it within the first 6 MB; a bigger preview simply causes a second read.
const FIRST_READ: u64 = 8 * 1024 * 1024;

/// Whether `data` (the start of a file is enough) is a Fujifilm RAF.
pub fn is_raf(data: &[u8]) -> bool {
    craft_raw::probe(data) == Some(craft_raw::RawFormat::Raf)
}

/// The preview JPEG the camera embedded when it wrote the RAF.
/// This is the as-shot look and needs no camera, so it is used for thumbnails.
pub fn read_embedded_jpeg(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let mut start = Vec::new();
    File::open(path)?.take(FIRST_READ).read_to_end(&mut start)?;
    if !is_raf(&start) {
        return Ok(None);
    }
    let Some(range) = craft_raw::bounded_preview(&start) else { return Ok(None) };
    // `bounded_preview` stops at the end of what it was given. If the preview
    // reaches that far it may go on, so read the whole file to be sure.
    let reaches_end = range.offset + range.window_len >= start.len() as u64;
    if reaches_end && start.len() as u64 == FIRST_READ {
        return Ok(craft_raw::embedded_jpeg_preview(&std::fs::read(path)?));
    }
    let window = &start[range.offset as usize..];
    Ok(craft_raw::embedded_preview_from_range(window, range))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A RAF-shaped file: header, then a tiny JPEG where the header says.
    fn fake_raf(jpeg: &[u8]) -> Vec<u8> {
        let mut f = vec![0u8; 148];
        f[..16].copy_from_slice(b"FUJIFILMCCD-RAW ");
        f[16..20].copy_from_slice(b"0201");
        f[28..32].copy_from_slice(b"X-T4");
        f[84..88].copy_from_slice(&148u32.to_be_bytes());
        f[88..92].copy_from_slice(&(jpeg.len() as u32).to_be_bytes());
        f.extend_from_slice(jpeg);
        f
    }

    #[test]
    fn knows_a_raf_when_it_sees_one() {
        assert!(is_raf(&fake_raf(&[0xFF, 0xD8, 0xFF, 0xD9])));
        assert!(!is_raf(b"\xFF\xD8\xFF\xE0 a jpeg"));
        assert!(!is_raf(b""));
    }
}
