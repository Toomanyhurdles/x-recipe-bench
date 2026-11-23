//! Background thread that decodes the pictures embedded in RAF files.
//! These need no camera, so the library can be browsed without one.

use crate::events::{AppEvent, EmbeddedKind, EventSender};
use eframe::egui::ColorImage;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

pub const THUMBNAIL_SIDE: u32 = 320;
pub const LARGE_SIDE: u32 = 2400;

pub struct ImageRequest {
    pub raf: PathBuf,
    pub kind: EmbeddedKind,
}

pub fn spawn(events: EventSender) -> Sender<ImageRequest> {
    let (tx, rx) = std::sync::mpsc::channel::<ImageRequest>();
    std::thread::Builder::new()
        .name("images".into())
        .spawn(move || {
            while let Ok(request) = rx.recv() {
                let side = match request.kind {
                    EmbeddedKind::Thumbnail => THUMBNAIL_SIDE,
                    EmbeddedKind::Large => LARGE_SIDE,
                };
                let image = load(&request.raf, side);
                events.send(AppEvent::Embedded { raf: request.raf, kind: request.kind, image });
            }
        })
        .expect("could not start the image thread");
    tx
}

fn load(raf: &Path, max_side: u32) -> Option<ColorImage> {
    let jpeg = xrp_camera::raf::read_embedded_jpeg(raf).ok()??;
    // The fast reduced decode is enough when only a thumbnail is wanted.
    let img = crate::jpeg::decode_at_least(&jpeg, max_side).ok()?;
    // Photos taken with the camera turned are stored sideways with a note
    // saying so; turn them upright here.
    let recorded = xrp_camera::exif::parse_jpeg(&jpeg).map(|e| e.orientation).unwrap_or(1);
    let img = crate::orientation::Orientation::from_exif(recorded).apply(img);
    let rgb = crate::resize::fit(&img.to_rgb8(), max_side, crate::resize::Quality::Fast);
    Some(ColorImage::from_rgb([rgb.width() as usize, rgb.height() as usize], rgb.as_raw()))
}
