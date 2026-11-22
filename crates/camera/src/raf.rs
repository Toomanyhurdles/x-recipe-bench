//! Reading facts from a RAF file on disk. No camera needed.
//!
//! RAF header (big-endian), as widely documented:
//! - 0..16   "FUJIFILMCCD-RAW "
//! - 28..60  camera model, zero padded
//! - 84..88  offset of the embedded preview JPEG
//! - 88..92  length of the embedded preview JPEG

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const MAGIC: &[u8] = b"FUJIFILMCCD-RAW";
const HEADER_LEN: usize = 92;

pub fn is_raf(data: &[u8]) -> bool {
    data.starts_with(MAGIC)
}

#[derive(Debug, Clone, PartialEq)]
pub struct RafHeader {
    /// Camera model as written by the camera, e.g. "X-T4".
    pub model: String,
    pub jpeg_offset: u64,
    pub jpeg_len: u64,
}

pub fn parse_header(header: &[u8]) -> Option<RafHeader> {
    if header.len() < HEADER_LEN || !is_raf(header) {
        return None;
    }
    let model_bytes = &header[28..60];
    let end = model_bytes.iter().position(|b| *b == 0).unwrap_or(model_bytes.len());
    let model = String::from_utf8_lossy(&model_bytes[..end]).trim().to_string();
    let be = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap()) as u64;
    Some(RafHeader { model, jpeg_offset: be(84), jpeg_len: be(88) })
}

pub fn read_header(path: &Path) -> std::io::Result<Option<RafHeader>> {
    let mut f = File::open(path)?;
    let mut header = [0u8; HEADER_LEN];
    if f.read_exact(&mut header).is_err() {
        return Ok(None);
    }
    Ok(parse_header(&header))
}

/// The preview JPEG the camera embedded when it wrote the RAF.
/// This is the as-shot look and needs no camera, so it is used for thumbnails.
pub fn read_embedded_jpeg(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let Some(h) = read_header(path)? else { return Ok(None) };
    let file_len = std::fs::metadata(path)?.len();
    if h.jpeg_len == 0 || h.jpeg_offset + h.jpeg_len > file_len {
        return Ok(None);
    }
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(h.jpeg_offset))?;
    let mut jpeg = vec![0u8; h.jpeg_len as usize];
    f.read_exact(&mut jpeg)?;
    if jpeg.starts_with(&[0xFF, 0xD8]) {
        Ok(Some(jpeg))
    } else {
        Ok(None)
    }
}
