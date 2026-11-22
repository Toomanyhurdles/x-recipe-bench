//! A simulated camera: answers the protocol the way an X-T4 in USB RAW
//! CONV./BACKUP RESTORE mode is documented to, without any hardware.
//!
//! Used by the automated tests, and by the app when the environment
//! variable `XRP_SIMULATED_CAMERA` is set, so screens that need a camera
//! (such as Camera Slots) can be tried without one. Its "developed photo"
//! is a few placeholder bytes, not a real picture.

use crate::error::Error;
use crate::ptp::{self, encode_string};
use crate::transport::Transport;
use std::collections::VecDeque;
use std::time::Duration;

pub const FAKE_JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 4, 0xFF, 0xD9];

#[derive(Default)]
pub struct FakeCamera {
    /// Bytes waiting to be read by the host, one entry per USB transfer.
    pub outbox: VecDeque<Vec<u8>>,
    /// A command waiting for its data phase: (code, transaction, params).
    pub awaiting_data: Option<(u16, u32, Vec<u32>)>,
    pub inbox: Vec<u8>,
    pub uploaded: Option<Vec<u8>>,
    pub announced_size: u32,
    pub profile: Vec<u8>,
    pub objects: Vec<u32>,
    pub next_handle: u32,
    pub foreign_raf: bool,
    pub mode: u16,
    pub log: Vec<String>,
    /// Glue the response onto the data transfer, as some devices do.
    pub glue_response: bool,
    /// Custom slots: name and settings by property code. Empty = a body
    /// without the slot commands.
    pub slots: Vec<(String, std::collections::BTreeMap<u16, u16>)>,
    /// Which slot the slot properties address (1 = C1).
    pub selected: u16,
}

impl FakeCamera {
    pub fn new() -> Self {
        FakeCamera { next_handle: 6, mode: 6, profile: vec![7u8; 621], ..Default::default() }
    }

    /// A camera with seven custom slots, behaving as real bodies are
    /// documented to: Provia, grain off (stored as 7), D-Range Priority off.
    pub fn with_slots() -> Self {
        let mut camera = FakeCamera::new();
        let settings: std::collections::BTreeMap<u16, u16> = (0xD18E..=0xD1A4u16)
            .map(|code| {
                let value = match code {
                    0xD18E => 7,
                    0xD18F => 2,
                    0xD190 => 100,
                    0xD192 => 1,
                    0xD195 => 7,
                    0xD196..=0xD198 => 1,
                    0xD199 => 2,
                    0xD19C => 10000,
                    0xD1A1 => 0x2000,
                    0xD1A3 | 0xD1A4 => 1,
                    _ => 0,
                };
                (code, value)
            })
            .collect();
        camera.slots = (1..=7).map(|n| (format!("CUSTOM {n}"), settings.clone())).collect();
        camera.selected = 3;
        camera
    }

    /// The camera's answer to a write of `value` to setting `code` of the
    /// chosen slot, applying the documented state rules.
    fn write_slot_setting(&mut self, code: u16, value: u16) -> u16 {
        let index = self.selected as usize - 1;
        let slot = &mut self.slots[index].1;
        let film = slot[&0xD192];
        let mono = matches!(film, 6..=10 | 12..=15);
        let toning = mono && film != 10;
        let priority_on = slot[&0xD191] != 0;
        let refused = match code {
            0xD190 | 0xD19D | 0xD19E => priority_on,
            0xD193 | 0xD194 => !toning || (value as i16) % 10 != 0,
            0xD195 => !(1..=5).contains(&value),
            0xD19C => slot[&0xD199] != 0x8007,
            0xD19F => mono,
            _ => false,
        };
        if refused {
            return 0x201C;
        }
        // Grain off is stored as 6 or 7, remembering the last grain size.
        let stored = match (code, value) {
            (0xD195, 1) => match slot[&0xD195] {
                2 | 3 | 6 => 6,
                _ => 7,
            },
            _ => value,
        };
        slot.insert(code, stored);
        0x2001
    }

    fn container(kind: u16, code: u16, tid: u32, payload: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&((12 + payload.len()) as u32).to_le_bytes());
        b.extend_from_slice(&kind.to_le_bytes());
        b.extend_from_slice(&code.to_le_bytes());
        b.extend_from_slice(&tid.to_le_bytes());
        b.extend_from_slice(payload);
        b
    }

    fn respond(&mut self, tid: u32, code: u16, data: Option<Vec<u8>>, op: u16) {
        let response = Self::container(3, code, tid, &[]);
        match data {
            Some(d) => {
                let mut data_c = Self::container(2, op, tid, &d);
                if self.glue_response {
                    data_c.extend_from_slice(&response);
                    self.outbox.push_back(data_c);
                } else {
                    self.outbox.push_back(data_c);
                    self.outbox.push_back(response);
                }
            }
            None => self.outbox.push_back(response),
        }
    }

    fn device_info() -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&100u16.to_le_bytes());
        d.extend_from_slice(&6u32.to_le_bytes());
        d.extend_from_slice(&100u16.to_le_bytes());
        d.extend(encode_string("fujifilm.co.jp: 1.0; "));
        d.extend_from_slice(&0u16.to_le_bytes());
        let arr = |v: &[u16]| {
            let mut b = (v.len() as u32).to_le_bytes().to_vec();
            for x in v {
                b.extend_from_slice(&x.to_le_bytes());
            }
            b
        };
        d.extend(arr(&[0x1001, 0x1002, 0x900C, 0x900D]));
        d.extend(arr(&[0x4002]));
        d.extend(arr(&[0xD212]));
        d.extend(arr(&[]));
        d.extend(arr(&[0x3801]));
        d.extend(encode_string("FUJIFILM"));
        d.extend(encode_string("X-T4"));
        d.extend(encode_string("2.12"));
        d.extend(encode_string("SERIAL"));
        d
    }

    fn handle_command(&mut self, code: u16, tid: u32, params: Vec<u32>, data: Option<Vec<u8>>) {
        self.log.push(format!("{code:04X}"));
        match code {
            0x1002 => self.respond(tid, 0x2001, None, code),
            0x1003 => self.respond(tid, 0x2001, None, code),
            0x1001 => self.respond(tid, 0x2001, Some(Self::device_info()), code),
            0x900C => {
                let info = ptp::decode_object_info(&data.unwrap()).unwrap();
                assert_eq!(info.format, 0xF802);
                assert_eq!(info.filename, "FUP_FILE.dat");
                assert_eq!(params, vec![0, 0, 0]);
                self.announced_size = info.size;
                self.respond(tid, 0x2001, None, code);
            }
            0x900D => {
                let d = data.unwrap();
                assert_eq!(d.len() as u32, self.announced_size, "upload size matches announcement");
                self.uploaded = Some(d);
                self.respond(tid, 0x2001, None, code);
            }
            0x1015 => match params[0] {
                0xD16E => self.respond(tid, 0x2001, Some(self.mode.to_le_bytes().to_vec()), code),
                0xD185 if self.uploaded.is_some() && !self.foreign_raf => {
                    self.respond(tid, 0x2001, Some(self.profile.clone()), code)
                }
                0xD185 => self.respond(tid, 0x2002, None, code),
                0xD18C if !self.slots.is_empty() => {
                    self.respond(tid, 0x2001, Some(self.selected.to_le_bytes().to_vec()), code)
                }
                0xD18D if !self.slots.is_empty() => {
                    let name = encode_string(&self.slots[self.selected as usize - 1].0);
                    self.respond(tid, 0x2001, Some(name), code)
                }
                prop @ 0xD18E..=0xD1A4 if !self.slots.is_empty() => {
                    let value = self.slots[self.selected as usize - 1].1[&(prop as u16)];
                    self.respond(tid, 0x2001, Some(value.to_le_bytes().to_vec()), code)
                }
                _ => self.respond(tid, 0x200A, None, code),
            },
            0x1016 => match params[0] {
                0xD185 => {
                    self.profile = data.unwrap();
                    self.respond(tid, 0x2001, None, code);
                }
                0xD183 => {
                    let v = u16::from_le_bytes(data.unwrap()[..2].try_into().unwrap());
                    if v > 1 || self.uploaded.is_none() {
                        self.respond(tid, 0x201C, None, code);
                    } else {
                        self.objects.push(self.next_handle);
                        self.next_handle += 1;
                        self.respond(tid, 0x2001, None, code);
                    }
                }
                0xD18C if !self.slots.is_empty() => {
                    let v = u16::from_le_bytes(data.unwrap()[..2].try_into().unwrap());
                    if (1..=self.slots.len() as u16).contains(&v) {
                        self.selected = v;
                        self.respond(tid, 0x2001, None, code);
                    } else {
                        self.respond(tid, 0x200A, None, code);
                    }
                }
                0xD18D if !self.slots.is_empty() => {
                    let d = data.unwrap();
                    let name = ptp::Reader::new(&d).string().unwrap();
                    self.slots[self.selected as usize - 1].0 = name;
                    self.respond(tid, 0x2001, None, code);
                }
                prop @ 0xD18E..=0xD1A4 if !self.slots.is_empty() => {
                    let v = u16::from_le_bytes(data.unwrap()[..2].try_into().unwrap());
                    let answer = self.write_slot_setting(prop as u16, v);
                    self.respond(tid, answer, None, code);
                }
                _ => self.respond(tid, 0x200A, None, code),
            },
            0x1007 => {
                let mut d = (self.objects.len() as u32).to_le_bytes().to_vec();
                for h in &self.objects {
                    d.extend_from_slice(&h.to_le_bytes());
                }
                self.respond(tid, 0x2001, Some(d), code);
            }
            0x1008 => {
                let d = ptp::encode_object_info(0x3801, FAKE_JPEG.len() as u32, "DSCF0006.jpg");
                self.respond(tid, 0x2001, Some(d), code);
            }
            0x1009 => self.respond(tid, 0x2001, Some(FAKE_JPEG.to_vec()), code),
            0x100B => {
                self.objects.retain(|h| *h != params[0]);
                self.respond(tid, 0x2001, None, code);
            }
            _ => self.respond(tid, 0x2005, None, code),
        }
    }
}

impl Transport for FakeCamera {
    fn write(&mut self, data: &[u8], _t: Duration) -> crate::Result<()> {
        self.inbox.extend_from_slice(data);
        while self.inbox.len() >= 12 {
            let len = u32::from_le_bytes(self.inbox[0..4].try_into().unwrap()) as usize;
            if self.inbox.len() < len {
                break;
            }
            let c: Vec<u8> = self.inbox.drain(..len).collect();
            let kind = u16::from_le_bytes([c[4], c[5]]);
            let code = u16::from_le_bytes([c[6], c[7]]);
            let tid = u32::from_le_bytes(c[8..12].try_into().unwrap());
            match kind {
                1 => {
                    let params: Vec<u32> = c[12..]
                        .chunks(4)
                        .map(|p| u32::from_le_bytes(p.try_into().unwrap()))
                        .collect();
                    let needs_data = matches!(code, 0x900C | 0x900D | 0x1016);
                    if needs_data {
                        self.awaiting_data = Some((code, tid, params));
                    } else {
                        self.handle_command(code, tid, params, None);
                    }
                }
                2 => {
                    let (cmd, cmd_tid, params) = self.awaiting_data.take().expect("data without command");
                    assert_eq!(cmd, code, "data phase carries the command's code");
                    assert_eq!(cmd_tid, tid, "data phase carries the command's transaction id");
                    self.handle_command(cmd, tid, params, Some(c[12..].to_vec()));
                }
                k => panic!("host sent container type {k}"),
            }
        }
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], _t: Duration) -> crate::Result<usize> {
        match self.outbox.pop_front() {
            Some(d) => {
                buf[..d.len()].copy_from_slice(&d);
                Ok(d.len())
            }
            None => Err(Error::Timeout("read")),
        }
    }

    fn packet_size(&self) -> usize {
        512
    }
}

