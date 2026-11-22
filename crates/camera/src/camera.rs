//! The camera as the app sees it: connect, upload a RAF, develop it.

use crate::error::{Error, Result};
use crate::ptp::{self, format, op, prop, rc, Ptp, Reader, LONG};
use crate::transport::{Transport, UsbTransport};
use std::time::{Duration, Instant};

/// What the camera says about itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceInfo {
    pub maker: String,
    pub model: String,
    pub firmware: String,
    pub operations: Vec<u16>,
    pub properties: Vec<u16>,
}

impl DeviceInfo {
    fn parse(data: &[u8]) -> Option<Self> {
        let mut r = Reader::new(data);
        r.skip(2 + 4 + 2); // standard version, vendor extension id + version
        let _extension = r.string()?;
        r.skip(2); // functional mode
        let operations = r.array_u16()?;
        let _events = r.array_u16()?;
        let properties = r.array_u16()?;
        let _capture_formats = r.array_u16()?;
        let _image_formats = r.array_u16()?;
        let maker = r.string()?;
        let model = r.string()?;
        let firmware = r.string()?;
        Some(DeviceInfo { maker, model, firmware, operations, properties })
    }

    /// True when the camera offers the two upload commands conversion needs.
    pub fn can_convert(&self) -> bool {
        self.operations.contains(&op::FUJI_SEND_OBJECT_INFO)
            && self.operations.contains(&op::FUJI_SEND_OBJECT)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UsbMode {
    Tether,
    RawConversion,
    Webcam,
    /// The camera answered with a value we do not know.
    Other(u16),
    /// The camera would not say.
    Unknown,
}

impl UsbMode {
    fn from_value(v: u16) -> Self {
        match v {
            5 => UsbMode::Tether,
            6 => UsbMode::RawConversion,
            8 => UsbMode::Webcam,
            v => UsbMode::Other(v),
        }
    }
    pub fn label(&self) -> String {
        match self {
            UsbMode::Tether => "USB tether shooting".into(),
            UsbMode::RawConversion => "USB RAW CONV./BACKUP RESTORE".into(),
            UsbMode::Webcam => "USB webcam".into(),
            UsbMode::Other(v) => format!("unrecognised (value {v})"),
            UsbMode::Unknown => "unknown".into(),
        }
    }
}

/// How large a developed image to ask for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderSize {
    /// Reduced size: quicker, for previews.
    Preview,
    /// Full resolution: for export.
    Full,
}

pub struct Camera<T: Transport> {
    pub(crate) ptp: Ptp<T>,
    pub info: DeviceInfo,
    pub mode: UsbMode,
}

impl Camera<UsbTransport> {
    /// Finds a Fujifilm camera on USB and starts talking to it.
    pub fn connect() -> Result<Self> {
        Camera::start(UsbTransport::open()?)
    }
}

impl<T: Transport> Camera<T> {
    /// Opens a session over an already-open transport and reads the basics.
    pub fn start(transport: T) -> Result<Self> {
        let mut ptp = Ptp::new(transport);
        ptp.transport.drain();

        // Another program may have left the camera mid-conversation, so the
        // first request gets two recovery attempts.
        let mut opened = None;
        for attempt in 1..=3u8 {
            ptp.reset_transaction_counter();
            match ptp.ask(op::OPEN_SESSION, &[1]) {
                Ok(reply) => {
                    opened = Some(reply.code);
                    break;
                }
                Err(e) if attempt == 3 => return Err(e),
                Err(_) => {
                    ptp.transport.recover(attempt);
                    ptp.transport.drain();
                }
            }
        }
        match opened {
            Some(rc::OK) | Some(rc::SESSION_ALREADY_OPEN) => {}
            Some(code) => return Err(Error::Refused { what: "open a session", code }),
            None => return Err(Error::Timeout("opening a session")),
        }

        let data = ptp.ask(op::GET_DEVICE_INFO, &[])?.require("read camera details")?;
        let info = DeviceInfo::parse(&data)
            .ok_or_else(|| Error::Other("Could not understand the camera's details.".into()))?;

        let mut camera = Camera { ptp, info, mode: UsbMode::Unknown };
        camera.mode = camera.read_mode();
        Ok(camera)
    }

    fn read_mode(&mut self) -> UsbMode {
        match self.ptp.get_prop(prop::USB_MODE) {
            Ok(reply) if reply.ok() && !reply.data.is_empty() => {
                let v = if reply.data.len() >= 2 {
                    u16::from_le_bytes([reply.data[0], reply.data[1]])
                } else {
                    reply.data[0] as u16
                };
                UsbMode::from_value(v)
            }
            _ => UsbMode::Unknown,
        }
    }

    /// Errors unless the camera is in a mode that can convert.
    /// An unknown mode is allowed through: some bodies do not report it.
    pub fn require_conversion_mode(&self) -> Result<()> {
        match self.mode {
            UsbMode::RawConversion | UsbMode::Unknown => Ok(()),
            m => Err(Error::WrongMode(m.label())),
        }
    }

    /// Sends a RAF to the camera. It stays loaded until another is sent,
    /// so several conversions can follow one upload.
    pub fn upload_raf(&mut self, raf: &[u8]) -> Result<()> {
        if !crate::raf::is_raf(raf) {
            return Err(Error::BadRaf("That file is not a Fujifilm RAF.".into()));
        }
        let info = ptp::encode_object_info(format::RAF, raf.len() as u32, "FUP_FILE.dat");
        self.ptp
            .run(op::FUJI_SEND_OBJECT_INFO, &[0, 0, 0], Some(&info), ptp::SHORT)?
            .require("accept the file description")?;
        self.ptp
            .run(op::FUJI_SEND_OBJECT, &[], Some(raf), LONG)?
            .require("accept the RAF")?;
        Ok(())
    }

    /// The conversion profile for the uploaded RAF: its as-shot settings
    /// until [`write_profile`](Self::write_profile) changes them.
    pub fn read_profile(&mut self) -> Result<Vec<u8>> {
        let reply = self.ptp.get_prop(prop::RAW_CONV_PROFILE)?;
        if reply.code == rc::GENERAL_ERROR {
            return Err(Error::BadRaf(
                "The camera would not prepare this RAF. It only develops files shot on this \
                 exact camera body."
                    .into(),
            ));
        }
        reply.require("read the conversion profile")
    }

    pub fn write_profile(&mut self, profile: &[u8]) -> Result<()> {
        self.ptp
            .set_prop(prop::RAW_CONV_PROFILE, profile)?
            .require("accept the conversion profile")?;
        Ok(())
    }

    /// Develops the uploaded RAF with the current profile and returns the image.
    pub fn develop(&mut self, size: RenderSize) -> Result<Vec<u8>> {
        let before = self.ptp.object_handles()?;

        let value: u16 = match size {
            RenderSize::Preview => 0,
            RenderSize::Full => 1,
        };
        let started = Instant::now();
        let reply = self.ptp.set_prop(prop::START_RAW_CONVERSION, &value.to_le_bytes())?;
        if reply.code == rc::DEVICE_BUSY {
            return Err(Error::Timeout("the camera reports it is busy"));
        }
        reply.require("start developing")?;

        // The result shows up as a new object.
        let handle = loop {
            if started.elapsed() > Duration::from_secs(60) {
                return Err(Error::Timeout("no developed image after 60 seconds"));
            }
            std::thread::sleep(Duration::from_millis(150));
            let now = self.ptp.object_handles()?;
            if let Some(h) = now.into_iter().find(|h| !before.contains(h)) {
                break h;
            }
        };

        let info_data = self
            .ptp
            .ask(op::GET_OBJECT_INFO, &[handle])?
            .require("describe the developed image")?;
        let info = ptp::decode_object_info(&info_data);

        let image = self
            .ptp
            .run(op::GET_OBJECT, &[handle], None, LONG)?
            .require("send the developed image")?;
        if image.is_empty() {
            return Err(Error::Other("The camera sent an empty image.".into()));
        }

        // Remove the temporary result, but only if it is clearly ours: a new
        // object whose declared size matches what we received.
        let size_matches = info.as_ref().map(|i| i.size as usize == image.len()).unwrap_or(false);
        if size_matches {
            let _ = self.ptp.ask(op::DELETE_OBJECT, &[handle, 0]);
        }
        Ok(image)
    }

    /// Ends the session politely.
    pub fn close(mut self) {
        let _ = self.ptp.ask(op::CLOSE_SESSION, &[]);
    }

    /// Cheap check that the camera is still there.
    pub fn is_alive(&mut self) -> bool {
        self.ptp.get_prop(prop::USB_MODE).is_ok()
    }
}
