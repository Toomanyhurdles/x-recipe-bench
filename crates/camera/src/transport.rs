//! Moving bytes to and from the camera.
//!
//! Everything above this file talks to a [`Transport`], so the protocol code
//! can be tested against a simulated camera (see `tests/`).

use crate::busy;
use crate::error::{Error, Result};
use rusb::{Context, DeviceHandle, Direction, TransferType, UsbContext};
use std::time::Duration;

pub const FUJIFILM_VENDOR_ID: u16 = 0x04CB;

/// So the app can hold either a real or a simulated camera.
impl Transport for Box<dyn Transport + Send> {
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<()> {
        (**self).write(data, timeout)
    }
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        (**self).read(buf, timeout)
    }
    fn packet_size(&self) -> usize {
        (**self).packet_size()
    }
    fn drain(&mut self) -> usize {
        (**self).drain()
    }
    fn recover(&mut self, level: u8) {
        (**self).recover(level)
    }
}

pub trait Transport {
    /// Sends all of `data`.
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<()>;
    /// Reads one USB transfer into `buf`; returns the byte count (may be 0).
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize>;
    /// Largest packet the outgoing endpoint takes (512 on high-speed USB).
    fn packet_size(&self) -> usize;
    /// Discards anything a previous program left unread. Returns bytes dropped.
    fn drain(&mut self) -> usize {
        0
    }
    /// Tries to un-stick the camera. `level` 1 is gentle, 2 is a port reset.
    fn recover(&mut self, _level: u8) {}
}

pub struct UsbTransport {
    handle: DeviceHandle<Context>,
    interface: u8,
    ep_in: u8,
    ep_out: u8,
    packet: usize,
    /// USB product id, e.g. 0x02E6 for the X-T4.
    pub product_id: u16,
}

impl UsbTransport {
    /// Finds the first Fujifilm camera and claims its PTP interface.
    pub fn open() -> Result<Self> {
        let ctx = Context::new().map_err(|e| Error::Other(format!("Could not start USB: {e}")))?;
        let devices = ctx
            .devices()
            .map_err(|e| Error::Other(format!("Could not list USB devices: {e}")))?;

        for dev in devices.iter() {
            let Ok(desc) = dev.device_descriptor() else { continue };
            if desc.vendor_id() != FUJIFILM_VENDOR_ID {
                continue;
            }
            let cfg = dev
                .active_config_descriptor()
                .or_else(|_| dev.config_descriptor(0))
                .map_err(|e| Error::Other(format!("Could not read the camera's USB layout: {e}")))?;

            // Prefer the still-image (PTP, class 6) interface.
            let mut pick: Option<(u8, u8, u8, usize, bool)> = None;
            for iface in cfg.interfaces() {
                for alt in iface.descriptors() {
                    let (mut ep_in, mut ep_out, mut packet) = (None, None, 512usize);
                    for ep in alt.endpoint_descriptors() {
                        if ep.transfer_type() != TransferType::Bulk {
                            continue;
                        }
                        match ep.direction() {
                            Direction::In => ep_in = ep_in.or(Some(ep.address())),
                            Direction::Out => {
                                if ep_out.is_none() {
                                    ep_out = Some(ep.address());
                                    packet = (ep.max_packet_size() as usize).max(8);
                                }
                            }
                        }
                    }
                    if let (Some(i), Some(o)) = (ep_in, ep_out) {
                        let is_ptp = alt.class_code() == 6;
                        if pick.is_none() || (is_ptp && !pick.unwrap().4) {
                            pick = Some((alt.interface_number(), i, o, packet, is_ptp));
                        }
                    }
                }
            }
            let Some((interface, ep_in, ep_out, packet, _)) = pick else { continue };

            let handle = dev.open().map_err(|e| match e {
                rusb::Error::Access => Error::Permission,
                rusb::Error::NotSupported => Error::Other(
                    "This system's driver for the camera does not allow direct USB access."
                        .into(),
                ),
                e => Error::Other(format!("Could not open the camera: {e}")),
            })?;
            let _ = handle.set_auto_detach_kernel_driver(true);
            handle.claim_interface(interface).map_err(|e| match e {
                rusb::Error::Busy => Error::Busy {
                    holder: busy::who_holds(dev.bus_number(), dev.address()),
                },
                e => Error::Other(format!("Could not take control of the camera: {e}")),
            })?;

            return Ok(UsbTransport {
                handle,
                interface,
                ep_in,
                ep_out,
                packet,
                product_id: desc.product_id(),
            });
        }
        Err(Error::NoCamera)
    }
}

impl Drop for UsbTransport {
    fn drop(&mut self) {
        let _ = self.handle.release_interface(self.interface);
    }
}

fn usb_err(e: rusb::Error, what: &'static str) -> Error {
    match e {
        rusb::Error::Timeout => Error::Timeout(what),
        rusb::Error::NoDevice => {
            Error::Other("The camera was disconnected or switched off.".into())
        }
        e => Error::Other(format!("USB {what} failed: {e}")),
    }
}

impl Transport for UsbTransport {
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<()> {
        // Chunks are a multiple of the packet size, so only the last can be short.
        for chunk in data.chunks(1024 * 1024) {
            let mut off = 0;
            while off < chunk.len() {
                off += self
                    .handle
                    .write_bulk(self.ep_out, &chunk[off..], timeout)
                    .map_err(|e| usb_err(e, "write"))?;
            }
        }
        // A transfer that ends exactly on a packet boundary needs an empty packet to end it.
        if !data.is_empty() && data.len().is_multiple_of(self.packet) {
            let _ = self.handle.write_bulk(self.ep_out, &[], Duration::from_secs(2));
        }
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        self.handle
            .read_bulk(self.ep_in, buf, timeout)
            .map_err(|e| usb_err(e, "read"))
    }

    fn packet_size(&self) -> usize {
        self.packet
    }

    fn drain(&mut self) -> usize {
        let mut buf = vec![0u8; 65536];
        let mut total = 0;
        while let Ok(n) = self
            .handle
            .read_bulk(self.ep_in, &mut buf, Duration::from_millis(150))
        {
            total += n;
            if n == 0 || total > 64 * 1024 * 1024 {
                break;
            }
        }
        total
    }

    fn recover(&mut self, level: u8) {
        let t = Duration::from_secs(5);
        if level <= 1 {
            // PTP class request "Device Reset", then clear both endpoints.
            let _ = self
                .handle
                .write_control(0x21, 0x66, 0, self.interface as u16, &[], t);
            let _ = self.handle.clear_halt(self.ep_in);
            let _ = self.handle.clear_halt(self.ep_out);
        } else {
            let _ = self.handle.reset();
            std::thread::sleep(Duration::from_millis(1500));
            let _ = self.handle.claim_interface(self.interface);
        }
        std::thread::sleep(Duration::from_millis(400));
    }
}
