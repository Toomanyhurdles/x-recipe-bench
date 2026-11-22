//! The conversion profile: the block of settings the camera uses to develop
//! the uploaded RAF (device property 0xD185).
//!
//! Layout, from the public fujifilm-ptp-reference notes:
//! - bytes 0..2   number of parameters (meaning differs when reading/writing)
//! - byte  2      character count of the profile code
//! - bytes 3..    the profile code as UTF-16, then zero padding
//! - from 0x1FD   the settings table: 4-byte signed integers ("cells")
//!
//! Older cameras have fewer cells; the X-T4 has 28.

/// Where the settings table starts in every profile seen so far.
const TABLE_START: usize = 0x1FD;
/// Length of the form the camera expects to be sent (30 cells).
const HOST_WRITE_LEN: usize = 629;

/// Positions of settings in the table.
pub mod cell {
    pub const EXPOSURE_BIAS: usize = 5;
    pub const DYNAMIC_RANGE: usize = 6;
    pub const FILM_SIMULATION: usize = 8;
    pub const GRAIN: usize = 9;
    pub const COLOR_CHROME: usize = 10;
    /// 1 = use the photo's as-shot white balance, 2 = use WHITE_BALANCE.
    pub const WB_SOURCE: usize = 11;
    pub const WHITE_BALANCE: usize = 12;
    pub const WB_SHIFT_RED: usize = 13;
    pub const WB_SHIFT_BLUE: usize = 14;
    pub const COLOR_TEMPERATURE: usize = 15;
    pub const HIGHLIGHT: usize = 16;
    pub const SHADOW: usize = 17;
    pub const COLOR: usize = 18;
    pub const SHARPNESS: usize = 19;
    pub const NOISE_REDUCTION: usize = 20;
    pub const LENS_OPTIMIZER: usize = 21;
    pub const COLOR_SPACE: usize = 22;
    pub const MONO_WARM_COOL: usize = 23;
    pub const SMOOTH_SKIN: usize = 24;
    pub const COLOR_CHROME_BLUE: usize = 25;
    pub const MONO_MAGENTA_GREEN: usize = 26;
    pub const CLARITY: usize = 27;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    bytes: Vec<u8>,
}

impl Profile {
    /// Wraps bytes read from the camera. `None` if they are too short to be a profile.
    pub fn from_bytes(bytes: Vec<u8>) -> Option<Self> {
        if bytes.len() < TABLE_START + 4 {
            return None;
        }
        Some(Profile { bytes })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn cell_count(&self) -> usize {
        (self.bytes.len() - TABLE_START) / 4
    }

    pub fn get(&self, index: usize) -> Option<i32> {
        if index >= self.cell_count() {
            return None;
        }
        let at = TABLE_START + 4 * index;
        Some(i32::from_le_bytes(self.bytes[at..at + 4].try_into().unwrap()))
    }

    /// Sets a cell. Returns false (and changes nothing) if this camera has no such cell.
    pub fn set(&mut self, index: usize, value: i32) -> bool {
        if index >= self.cell_count() {
            return false;
        }
        let at = TABLE_START + 4 * index;
        self.bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        true
    }

    /// The camera's code for this profile dialect, e.g. "FF159505".
    pub fn code(&self) -> String {
        let count = self.bytes[2] as usize;
        let units: Vec<u16> = self.bytes[3..]
            .chunks_exact(2)
            .take(count)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|u| *u != 0)
            .collect();
        String::from_utf16_lossy(&units)
    }

    /// The form to send back to the camera: padded with zeros to 30 cells,
    /// with the header adjusted the way Fujifilm's own software does it.
    pub fn to_host_write(&self) -> Vec<u8> {
        let mut out = self.bytes.clone();
        if out.len() < HOST_WRITE_LEN {
            out.resize(HOST_WRITE_LEN, 0);
        }
        let cells = (out.len() - TABLE_START) / 4;
        out[0..2].copy_from_slice(&((cells as u16).saturating_sub(1)).to_le_bytes());
        if out[2] == 8 {
            out[2] = 9; // count the terminating zero, as the camera expects on writes
        }
        out
    }

    /// Cells whose values differ between two profiles: (index, self, other).
    pub fn differences(&self, other: &Profile) -> Vec<(usize, i32, i32)> {
        (0..self.cell_count().min(other.cell_count()))
            .filter_map(|i| {
                let (a, b) = (self.get(i)?, other.get(i)?);
                (a != b).then_some((i, a, b))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(cells: usize) -> Profile {
        let mut b = vec![0u8; TABLE_START + 4 * cells];
        b[0..2].copy_from_slice(&(cells as u16).to_le_bytes());
        b[2] = 8;
        for (i, ch) in "FF159505".encode_utf16().enumerate() {
            b[3 + 2 * i..5 + 2 * i].copy_from_slice(&ch.to_le_bytes());
        }
        Profile::from_bytes(b).unwrap()
    }

    #[test]
    fn x_t4_sized_profile_has_28_cells() {
        let p = sample(28);
        assert_eq!(p.as_bytes().len(), 621);
        assert_eq!(p.cell_count(), 28);
        assert_eq!(p.code(), "FF159505");
    }

    #[test]
    fn cells_read_back_and_stay_in_bounds() {
        let mut p = sample(28);
        assert!(p.set(cell::FILM_SIMULATION, 11));
        assert!(p.set(cell::HIGHLIGHT, -15));
        assert_eq!(p.get(cell::FILM_SIMULATION), Some(11));
        assert_eq!(p.get(cell::HIGHLIGHT), Some(-15));
        assert!(!p.set(28, 1), "the X-T4 has no cell 28");
        assert_eq!(p.get(28), None);
    }

    #[test]
    fn host_write_form_is_629_bytes_with_adjusted_header() {
        let mut p = sample(28);
        p.set(cell::CLARITY, 30);
        let w = p.to_host_write();
        assert_eq!(w.len(), 629);
        assert_eq!(u16::from_le_bytes([w[0], w[1]]), 29);
        assert_eq!(w[2], 9);
        // Existing cells are untouched and the added ones are zero.
        assert_eq!(&w[3..621], &p.as_bytes()[3..621]);
        assert!(w[621..].iter().all(|b| *b == 0));
    }

    /// Checks against a real capture when a copy of libfuji is available:
    /// what an X-Pro3 sent, and what Fujifilm's software sent back.
    #[test]
    fn matches_a_real_capture_when_available() {
        let Some(dir) = std::env::var_os("XRP_LIBFUJI_DIR") else { return };
        let dir = std::path::PathBuf::from(dir);
        let find = |name: &str| -> Option<Vec<u8>> {
            fn walk(d: &std::path::Path, name: &str) -> Option<Vec<u8>> {
                for e in std::fs::read_dir(d).ok()?.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        if let Some(b) = walk(&p, name) {
                            return Some(b);
                        }
                    } else if p.file_name().map(|n| n == name).unwrap_or(false) {
                        return std::fs::read(p).ok();
                    }
                }
                None
            }
            walk(&dir, name)
        };
        let (Some(read), Some(written)) =
            (find("xpro3_d185_from_cam.bin"), find("xpro3_d185_to_cam.bin"))
        else {
            return;
        };
        let from_camera = Profile::from_bytes(read).unwrap();
        let to_camera = Profile::from_bytes(written.clone()).unwrap();
        // Apply the same setting changes the capture made, then compare every byte.
        let mut ours = from_camera.clone();
        for (i, _, theirs) in from_camera.differences(&to_camera) {
            ours.set(i, theirs);
        }
        assert_eq!(ours.to_host_write(), written);
    }
}
