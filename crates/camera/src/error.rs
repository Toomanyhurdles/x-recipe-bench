//! Errors, worded so they can be shown to the user as they are.

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// No Fujifilm camera on USB.
    NoCamera,
    /// The operating system would not let us open the camera.
    Permission,
    /// Another program has the camera open. `holder` names it when known.
    Busy { holder: Option<String> },
    /// The camera is connected but not in USB RAW CONV./BACKUP RESTORE mode.
    WrongMode(String),
    /// The camera did not answer in time.
    Timeout(&'static str),
    /// The camera answered a request with an error code.
    Refused { what: &'static str, code: u16 },
    /// The RAF is not usable (wrong file, or shot on another body).
    BadRaf(String),
    /// Anything else from USB or the protocol.
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoCamera => write!(
                f,
                "No Fujifilm camera found. Check the cable, switch the camera on, and set its \
                 USB mode to USB RAW CONV./BACKUP RESTORE."
            ),
            Error::Permission => write!(
                f,
                "The system refused access to the camera (permission denied)."
            ),
            Error::Busy { holder: Some(h) } => {
                write!(f, "The camera is in use by another program: {h}.")
            }
            Error::Busy { holder: None } => {
                write!(f, "The camera is in use by another program.")
            }
            Error::WrongMode(m) => write!(
                f,
                "The camera is in {m} mode. Set Connection Mode to USB RAW CONV./BACKUP RESTORE \
                 and reconnect it."
            ),
            Error::Timeout(what) => write!(
                f,
                "The camera did not answer ({what}). Switch it off and on, then reconnect."
            ),
            Error::Refused { what, code } => {
                write!(f, "The camera refused: {what} (code {code:#06x}).")
            }
            Error::BadRaf(m) => write!(f, "{m}"),
            Error::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}
