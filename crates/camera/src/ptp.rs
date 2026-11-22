//! Picture Transfer Protocol: framing, transactions, and dataset parsing.
//!
//! A transaction is: one command container out, an optional data container
//! (out or in), then one response container in. All integers are little-endian.

use crate::error::{Error, Result};
use crate::transport::Transport;
use std::time::Duration;

/// Operation codes (requests we send).
pub mod op {
    pub const GET_DEVICE_INFO: u16 = 0x1001;
    pub const OPEN_SESSION: u16 = 0x1002;
    pub const CLOSE_SESSION: u16 = 0x1003;
    pub const GET_OBJECT_HANDLES: u16 = 0x1007;
    pub const GET_OBJECT_INFO: u16 = 0x1008;
    pub const GET_OBJECT: u16 = 0x1009;
    pub const DELETE_OBJECT: u16 = 0x100B;
    pub const GET_DEVICE_PROP_VALUE: u16 = 0x1015;
    pub const SET_DEVICE_PROP_VALUE: u16 = 0x1016;
    /// Fujifilm: first half of a RAF upload (describes the file).
    pub const FUJI_SEND_OBJECT_INFO: u16 = 0x900C;
    /// Fujifilm: second half of a RAF upload (the file itself).
    pub const FUJI_SEND_OBJECT: u16 = 0x900D;
}

/// Response codes (answers we get).
pub mod rc {
    pub const OK: u16 = 0x2001;
    pub const GENERAL_ERROR: u16 = 0x2002;
    pub const PROP_NOT_SUPPORTED: u16 = 0x200A;
    pub const DEVICE_BUSY: u16 = 0x2019;
    pub const SESSION_ALREADY_OPEN: u16 = 0x201E;
}

/// Device property codes (camera settings).
pub mod prop {
    /// Which USB mode the camera is in. 6 = RAW CONV./BACKUP RESTORE.
    pub const USB_MODE: u16 = 0xD16E;
    /// Write 1 (full size) or 0 (reduced size) to start a conversion.
    pub const START_RAW_CONVERSION: u16 = 0xD183;
    /// The conversion profile (the "recipe" block) for the uploaded RAF.
    pub const RAW_CONV_PROFILE: u16 = 0xD185;
}

/// Object format codes.
pub mod format {
    pub const JPEG: u16 = 0x3801;
    pub const RAF: u16 = 0xF802;
}

const KIND_COMMAND: u16 = 1;
const KIND_DATA: u16 = 2;
const KIND_RESPONSE: u16 = 3;

pub const SHORT: Duration = Duration::from_secs(5);
pub const LONG: Duration = Duration::from_secs(60);

/// The answer to one transaction.
#[derive(Debug)]
pub struct Reply {
    pub code: u16,
    pub data: Vec<u8>,
}

impl Reply {
    pub fn ok(&self) -> bool {
        self.code == rc::OK
    }
    /// Turns a non-OK answer into an error naming what was being attempted.
    pub fn require(self, what: &'static str) -> Result<Vec<u8>> {
        if self.ok() {
            Ok(self.data)
        } else {
            Err(Error::Refused { what, code: self.code })
        }
    }
}

pub struct Ptp<T: Transport> {
    pub transport: T,
    transaction: u32,
    /// Bytes read past the end of a container (belong to the next one).
    pending: Vec<u8>,
}

impl<T: Transport> Ptp<T> {
    pub fn new(transport: T) -> Self {
        Ptp { transport, transaction: 0, pending: Vec::new() }
    }

    pub fn reset_transaction_counter(&mut self) {
        self.transaction = 0;
        self.pending.clear();
    }

    fn header(&self, len: usize, kind: u16, code: u16) -> Vec<u8> {
        let mut b = Vec::with_capacity(12);
        b.extend_from_slice(&(len as u32).to_le_bytes());
        b.extend_from_slice(&kind.to_le_bytes());
        b.extend_from_slice(&code.to_le_bytes());
        b.extend_from_slice(&self.transaction.to_le_bytes());
        b
    }

    fn read_container(&mut self, timeout: Duration) -> Result<(u16, u16, Vec<u8>)> {
        let mut buf = std::mem::take(&mut self.pending);
        let mut chunk = vec![0u8; 1024 * 1024];
        let mut empties = 0;
        loop {
            if buf.len() >= 12 {
                let len = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
                if !(12..=1 << 30).contains(&len) {
                    return Err(Error::Other(format!(
                        "The camera sent something unreadable (length {len})."
                    )));
                }
                if buf.len() >= len {
                    self.pending = buf.split_off(len);
                    let kind = u16::from_le_bytes([buf[4], buf[5]]);
                    let code = u16::from_le_bytes([buf[6], buf[7]]);
                    return Ok((kind, code, buf.split_off(12)));
                }
            }
            let n = self.transport.read(&mut chunk, timeout)?;
            if n == 0 {
                empties += 1;
                if empties > 8 {
                    return Err(Error::Timeout("only empty replies"));
                }
            }
            buf.extend_from_slice(&chunk[..n]);
        }
    }

    /// Runs one transaction. `send` is an optional outgoing data phase.
    pub fn run(
        &mut self,
        code: u16,
        params: &[u32],
        send: Option<&[u8]>,
        timeout: Duration,
    ) -> Result<Reply> {
        let mut cmd = self.header(12 + 4 * params.len(), KIND_COMMAND, code);
        for p in params {
            cmd.extend_from_slice(&p.to_le_bytes());
        }
        self.transport.write(&cmd, timeout)?;
        if let Some(d) = send {
            let mut out = self.header(12 + d.len(), KIND_DATA, code);
            out.extend_from_slice(d);
            self.transport.write(&out, timeout)?;
        }
        self.transaction = self.transaction.wrapping_add(1);

        let mut data = Vec::new();
        loop {
            let (kind, code, payload) = self.read_container(timeout)?;
            match kind {
                KIND_DATA => data = payload,
                KIND_RESPONSE => return Ok(Reply { code, data }),
                k => {
                    return Err(Error::Other(format!(
                        "The camera sent an unexpected reply (type {k})."
                    )))
                }
            }
        }
    }

    /// A transaction with no outgoing data and the short timeout.
    pub fn ask(&mut self, code: u16, params: &[u32]) -> Result<Reply> {
        self.run(code, params, None, SHORT)
    }

    pub fn get_prop(&mut self, prop: u16) -> Result<Reply> {
        self.ask(op::GET_DEVICE_PROP_VALUE, &[prop as u32])
    }

    pub fn set_prop(&mut self, prop: u16, value: &[u8]) -> Result<Reply> {
        self.run(op::SET_DEVICE_PROP_VALUE, &[prop as u32], Some(value), LONG)
    }

    /// Handles of every object the camera currently lists.
    pub fn object_handles(&mut self) -> Result<Vec<u32>> {
        let data = self
            .ask(op::GET_OBJECT_HANDLES, &[0xFFFF_FFFF, 0, 0])?
            .require("list images")?;
        let mut r = Reader::new(&data);
        let n = r.u32().unwrap_or(0) as usize;
        Ok((0..n).filter_map(|_| r.u32()).collect())
    }
}

/// Reads PTP-encoded values out of a byte slice.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }
    pub fn skip(&mut self, n: usize) {
        self.pos += n;
    }
    pub fn u8(&mut self) -> Option<u8> {
        let v = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }
    pub fn u16(&mut self) -> Option<u16> {
        let s = self.data.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(u16::from_le_bytes([s[0], s[1]]))
    }
    pub fn u32(&mut self) -> Option<u32> {
        let s = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes(s.try_into().unwrap()))
    }
    /// PTP string: a count byte, then that many UTF-16 units including a final zero.
    pub fn string(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        let mut units = Vec::with_capacity(n);
        for _ in 0..n {
            units.push(self.u16()?);
        }
        while units.last() == Some(&0) {
            units.pop();
        }
        Some(String::from_utf16_lossy(&units))
    }
    pub fn array_u16(&mut self) -> Option<Vec<u16>> {
        let n = self.u32()? as usize;
        if n > 10_000 {
            return None;
        }
        (0..n).map(|_| self.u16()).collect()
    }
}

/// Encodes a PTP string. An empty string is a single zero byte.
pub fn encode_string(s: &str) -> Vec<u8> {
    if s.is_empty() {
        return vec![0];
    }
    let units: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let mut b = vec![units.len() as u8];
    for u in units {
        b.extend_from_slice(&u.to_le_bytes());
    }
    b
}

/// Builds the ObjectInfo dataset that announces an upload.
pub fn encode_object_info(format: u16, size: u32, filename: &str) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&0u32.to_le_bytes()); // storage id
    b.extend_from_slice(&format.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes()); // protection status
    b.extend_from_slice(&size.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes()); // thumbnail format
    b.extend_from_slice(&[0u8; 28]); // thumbnail size/width/height, image width/height/depth, parent
    b.extend_from_slice(&0u16.to_le_bytes()); // association type
    b.extend_from_slice(&[0u8; 8]); // association description, sequence number
    b.extend(encode_string(filename));
    b.extend(encode_string("")); // capture date
    b.extend(encode_string("")); // modification date
    b.extend(encode_string("")); // keywords
    b
}

/// The parts of an ObjectInfo dataset we use.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectInfo {
    pub format: u16,
    pub size: u32,
    pub filename: String,
}

pub fn decode_object_info(data: &[u8]) -> Option<ObjectInfo> {
    let mut r = Reader::new(data);
    r.skip(4);
    let format = r.u16()?;
    r.skip(2);
    let size = r.u32()?;
    r.skip(2 + 28 + 2 + 8);
    let filename = r.string().unwrap_or_default();
    Some(ObjectInfo { format, size, filename })
}
