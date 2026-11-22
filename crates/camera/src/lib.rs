//! Camera layer for X Recipe Bench.
//!
//! Layers, bottom to top:
//! - [`transport`]: moves bytes to and from the camera (USB, or a fake for tests).
//! - [`ptp`]: the Picture Transfer Protocol: containers, transactions, datasets.
//! - [`camera`]: what the app uses: connect, upload a RAF, develop it.
//! - [`profile`]: the settings block the camera develops with.
//! - [`raf`]: reads facts out of a RAF file on disk (no camera needed).
//! - [`exif`]: reads the details a camera writes into a photo.
//! - [`busy`]: works out which other program is holding the camera.
//!
//! The conversion procedure follows the public "fujifilm-ptp-reference"
//! notes by zgrimz (CC BY 4.0) and the PTP standard (ISO 15740).

pub mod busy;
pub mod camera;
pub mod error;
pub mod exif;
pub mod profile;
pub mod ptp;
pub mod raf;
pub mod sim;
pub mod slots;
pub mod transport;

pub use camera::{Camera, DeviceInfo, RenderSize, UsbMode};
pub use error::{Error, Result};
