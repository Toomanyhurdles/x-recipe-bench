//! The camera's custom-setting slots (C1, C2, …): reading them, and writing
//! a name and settings into one.
//!
//! How it works, in USB RAW CONV./BACKUP RESTORE mode:
//! - `0xD18C` chooses which slot the following properties talk about. It is
//!   only the host's pointer; it does not change the slot the camera shoots with.
//! - `0xD18D` is the chosen slot's name, `0xD18E`…`0xD1A4` its settings,
//!   each a 16-bit number.
//!
//! This module moves raw numbers only. What they mean (which number is
//! Classic Chrome, and so on) lives in the recipes crate, `slot.rs`.
//!
//! The camera can refuse a write depending on the slot's state, and can
//! store a value differently from how it was written, so every write is
//! read back and both the camera's answer and the read-back are reported.
//!
//! Protocol knowledge: zgrimz/fujifilm-ptp-reference, chapters 3 and 4.

use crate::camera::Camera;
use crate::error::{Error, Result};
use crate::ptp::{encode_string, rc, Reader};
use crate::transport::Transport;
use std::time::Duration;

pub mod prop {
    /// Which slot the other properties address (1 = C1).
    pub const SELECT: u16 = 0xD18C;
    pub const NAME: u16 = 0xD18D;
    pub const IMAGE_SIZE: u16 = 0xD18E;
    pub const IMAGE_QUALITY: u16 = 0xD18F;
    pub const DYNAMIC_RANGE: u16 = 0xD190;
    pub const DR_PRIORITY: u16 = 0xD191;
    pub const FILM_SIMULATION: u16 = 0xD192;
    pub const MONO_WARM_COOL: u16 = 0xD193;
    pub const MONO_MAGENTA_GREEN: u16 = 0xD194;
    pub const GRAIN: u16 = 0xD195;
    pub const COLOR_CHROME: u16 = 0xD196;
    pub const COLOR_CHROME_BLUE: u16 = 0xD197;
    pub const SMOOTH_SKIN: u16 = 0xD198;
    pub const WHITE_BALANCE: u16 = 0xD199;
    pub const WB_SHIFT_RED: u16 = 0xD19A;
    pub const WB_SHIFT_BLUE: u16 = 0xD19B;
    pub const COLOR_TEMPERATURE: u16 = 0xD19C;
    pub const HIGHLIGHT: u16 = 0xD19D;
    pub const SHADOW: u16 = 0xD19E;
    pub const COLOR: u16 = 0xD19F;
    pub const SHARPNESS: u16 = 0xD1A0;
    pub const NOISE_REDUCTION: u16 = 0xD1A1;
    pub const CLARITY: u16 = 0xD1A2;
    pub const LENS_OPTIMIZER: u16 = 0xD1A3;
    pub const COLOR_SPACE: u16 = 0xD1A4;

    /// Every setting of a slot, in the order they are read.
    pub const SETTINGS: std::ops::RangeInclusive<u16> = IMAGE_SIZE..=COLOR_SPACE;
}

/// The longest slot name the camera's own name editor allows.
pub const NAME_LIMIT: usize = 25;

/// No known body has more slots than this; probing stops here.
const MOST_SLOTS: u16 = 10;

/// A short pause after choosing a slot. Other tools wait here as a
/// precaution; no failure without it is on record.
const SETTLE: Duration = Duration::from_millis(50);

/// What one slot holds, as raw numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotRead {
    /// 1 for C1.
    pub number: u16,
    pub name: String,
    /// Each setting's property code and value. `Err` holds the camera's
    /// answer code for a setting it would not give (not every body has all).
    pub values: Vec<(u16, std::result::Result<u16, u16>)>,
}

impl SlotRead {
    pub fn get(&self, prop: u16) -> Option<u16> {
        self.values.iter().find(|(p, _)| *p == prop).and_then(|(_, v)| v.ok())
    }
}

/// What happened to one setting that was written.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteOutcome {
    pub prop: u16,
    pub wanted: u16,
    /// The camera's answer to the write: `0x2001` is OK, `0x201C` means
    /// "not this value now", `0x200A` "no such setting on this camera".
    pub response: u16,
    /// The value the slot holds afterwards, if it could be read.
    pub read_back: Option<u16>,
}

/// The result of writing a slot.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotWrite {
    /// The camera's answer to the name write, when a name was sent.
    pub name_response: Option<u16>,
    /// The slot's name afterwards.
    pub name_after: String,
    pub outcomes: Vec<WriteOutcome>,
}

/// A name the camera accepts: plain printable characters, at most
/// [`NAME_LIMIT`] of them. Other characters are dropped.
pub fn clean_name(text: &str) -> String {
    let plain: String = text.chars().filter(|c| (' '..='~').contains(c)).collect();
    plain.trim().chars().take(NAME_LIMIT).collect::<String>().trim_end().to_string()
}

impl<T: Transport> Camera<T> {
    fn read_u16(&mut self, prop: u16) -> Result<std::result::Result<u16, u16>> {
        let reply = self.ptp.get_prop(prop)?;
        if !reply.ok() {
            return Ok(Err(reply.code));
        }
        Ok(Reader::new(&reply.data).u16().ok_or(rc::GENERAL_ERROR))
    }

    /// Points the slot properties at slot `number`. `Ok(false)` means the
    /// camera has no such slot.
    fn select_slot(&mut self, number: u16) -> Result<bool> {
        let reply = self.ptp.set_prop(prop::SELECT, &number.to_le_bytes())?;
        if !reply.ok() {
            return Ok(false);
        }
        std::thread::sleep(SETTLE);
        // Everything that follows lands on whichever slot is chosen, so make
        // sure the choice really took.
        match self.read_u16(prop::SELECT)? {
            Ok(now) if now == number => Ok(true),
            Ok(now) => Err(Error::Other(format!(
                "The camera did not switch to custom slot C{number} (it reports C{now}). Nothing was changed."
            ))),
            Err(code) => Err(Error::Refused { what: "checking the chosen custom slot", code }),
        }
    }

    fn read_selected_slot(&mut self, number: u16) -> Result<SlotRead> {
        let name = match self.ptp.get_prop(prop::NAME)? {
            reply if reply.ok() => Reader::new(&reply.data).string().unwrap_or_default(),
            _ => String::new(),
        };
        let mut values = Vec::new();
        for code in prop::SETTINGS {
            values.push((code, self.read_u16(code)?));
        }
        Ok(SlotRead { number, name, values })
    }

    /// Reads every custom slot. The number of slots is found by trying
    /// C1, C2, … until the camera says there is no such slot.
    pub fn read_slots(&mut self) -> Result<Vec<SlotRead>> {
        self.require_conversion_mode()?;
        // Remember where the pointer was, to leave it as it was found.
        let before = self.read_u16(prop::SELECT)?;
        let mut slots = Vec::new();
        for number in 1..=MOST_SLOTS {
            if !self.select_slot(number)? {
                break;
            }
            slots.push(self.read_selected_slot(number)?);
        }
        if slots.is_empty() {
            return Err(Error::Other(
                "This camera did not answer the custom-slot commands in this USB mode, so its slots cannot be \
                 read or written this way."
                    .into(),
            ));
        }
        if let Ok(original) = before {
            if (1..=slots.len() as u16).contains(&original) {
                self.select_slot(original)?;
            }
        }
        Ok(slots)
    }

    /// Reads one custom slot.
    pub fn read_slot(&mut self, number: u16) -> Result<SlotRead> {
        self.require_conversion_mode()?;
        if !self.select_slot(number)? {
            return Err(Error::Other(format!("The camera has no custom slot C{number}.")));
        }
        self.read_selected_slot(number)
    }

    /// Writes a name and settings into slot `number`, in the order given,
    /// then reads them back. A setting the camera refuses does not stop the
    /// rest; each outcome is reported. Settings not listed are untouched.
    pub fn write_slot(&mut self, number: u16, name: Option<&str>, writes: &[(u16, u16)]) -> Result<SlotWrite> {
        self.require_conversion_mode()?;
        if !self.select_slot(number)? {
            return Err(Error::Other(format!("The camera has no custom slot C{number}.")));
        }
        let name_response = match name {
            Some(name) => Some(self.ptp.set_prop(prop::NAME, &encode_string(&clean_name(name)))?.code),
            None => None,
        };
        let mut outcomes = Vec::new();
        for &(code, wanted) in writes {
            let response = self.ptp.set_prop(code, &wanted.to_le_bytes())?.code;
            outcomes.push(WriteOutcome { prop: code, wanted, response, read_back: None });
        }
        // Read back after all the writes: a later write can change what an
        // earlier one stored.
        for outcome in &mut outcomes {
            outcome.read_back = self.read_u16(outcome.prop)?.ok();
        }
        let name_after = match self.ptp.get_prop(prop::NAME)? {
            reply if reply.ok() => Reader::new(&reply.data).string().unwrap_or_default(),
            _ => String::new(),
        };
        Ok(SlotWrite { name_response, name_after, outcomes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_limited_to_what_the_camera_shows() {
        assert_eq!(clean_name("  Campfire  "), "Campfire");
        assert_eq!(clean_name("Café “Noir” ☕"), "Caf Noir");
        assert_eq!(clean_name("A very long recipe name that goes on"), "A very long recipe name t");
        assert_eq!(clean_name("abcdefghijklmnopqrstuvwx yz"), "abcdefghijklmnopqrstuvwx");
        assert_eq!(clean_name("日本"), "");
    }
}
