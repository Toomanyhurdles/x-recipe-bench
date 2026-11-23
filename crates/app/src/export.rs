//! How exported JPEGs are finished and named: resize, quality, file names.
//!
//! The camera makes the JPEG. The app only changes it when asked to turn
//! it, add a border, or resize it; otherwise the camera's file is saved
//! untouched. `Finish` gathers everything that is done to one export.

use crate::border::Border;
use crate::orientation::Orientation;
use serde::{Deserialize, Serialize};
use xrp_camera::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportOptions {
    /// JPEG quality used when the app has to save the picture again
    /// (turned, bordered or resized). 1–100.
    pub quality: u8,
    /// Make the picture smaller so its longer side is `long_edge` pixels.
    pub resize: bool,
    pub long_edge: u32,
    /// File name pattern. `{photo}` is the RAF's name without ".RAF",
    /// `{recipe}` the recipe's name.
    pub naming: String,
}

pub const DEFAULT_NAMING: &str = "{photo} - {recipe}";

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions { quality: 98, resize: false, long_edge: 3000, naming: DEFAULT_NAMING.into() }
    }
}

impl ExportOptions {
    /// The file name (with ".jpg") for a photo exported with a recipe, or
    /// with none (`recipe` = `None`), in which case the recipe part of the
    /// pattern and the separator next to it are dropped.
    pub fn file_name(&self, photo: &str, recipe: Option<&str>) -> String {
        let pattern = if self.naming.contains("{photo}") { self.naming.as_str() } else { DEFAULT_NAMING };
        let name = match recipe {
            Some(recipe) => pattern.replace("{recipe}", recipe),
            None => pattern
                .split("{recipe}")
                .map(|part| part.trim_matches([' ', '-', '_', ',', '(', ')']))
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
        };
        let name = name.replace("{photo}", photo);
        let safe: String =
            name.chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c }).collect();
        format!("{}.jpg", safe.trim_matches(['.', ' ']))
    }
}

/// Everything done to one exported picture after the camera made it.
#[derive(Debug, Clone, PartialEq)]
pub struct Finish {
    pub orientation: Orientation,
    /// Cut down to this part of the upright photo, after turning.
    pub crop: Option<crate::photo::Crop>,
    pub border: Border,
    pub options: ExportOptions,
}

impl Finish {
    /// The file to save. Untouched if nothing was asked for; otherwise the
    /// pixels are turned, cropped, resized, given the border (in that order) and
    /// saved again, keeping the camera's details (EXIF).
    pub fn apply(&self, jpeg: Vec<u8>) -> Result<Vec<u8>, Error> {
        let crop = self.crop.filter(|c| !c.is_none());
        if self.orientation.is_identity() && crop.is_none() && self.border.is_none() && !self.options.resize {
            return Ok(jpeg);
        }
        let recorded = xrp_camera::exif::parse_jpeg(&jpeg).map(|e| e.orientation).unwrap_or(1);
        let image = crate::jpeg::decode(&jpeg)
            .map_err(|e| Error::Other(format!("Could not read the developed image: {e}")))?;
        let mut image = self.orientation.apply(Orientation::from_exif(recorded).apply(image)).to_rgb8();
        if let Some(crop) = crop {
            image = crop.apply(image);
        }
        if self.options.resize {
            let long = self.options.long_edge.clamp(64, 20000);
            let (w, h) = (image.width(), image.height());
            // Only ever make it smaller.
            if w.max(h) > long {
                let scale = long as f32 / w.max(h) as f32;
                let (nw, nh) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
                image = crate::resize::resize(&image, nw, nh, crate::resize::Quality::Best);
            }
        }
        let image = self.border.add(image);
        crate::jpeg::encode(&image, self.options.quality, Some(&jpeg))
            .map_err(|e| Error::Other(format!("Could not save the finished image: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_follow_the_pattern() {
        let o = ExportOptions::default();
        assert_eq!(o.file_name("DSCF2002", Some("Campfire")), "DSCF2002 - Campfire.jpg");
        assert_eq!(o.file_name("DSCF2002", None), "DSCF2002.jpg");
        let o = ExportOptions { naming: "{recipe}_{photo}".into(), ..Default::default() };
        assert_eq!(o.file_name("DSCF2002", Some("A/B: test")), "A_B_ test_DSCF2002.jpg");
        assert_eq!(o.file_name("DSCF2002", None), "DSCF2002.jpg");
        // A pattern without the photo's name would give every file the same name.
        let o = ExportOptions { naming: "export".into(), ..Default::default() };
        assert_eq!(o.file_name("DSCF2002", Some("Campfire")), "DSCF2002 - Campfire.jpg");
    }

    #[test]
    fn resizing_shrinks_to_the_long_edge_and_never_enlarges() {
        let img = image::RgbImage::from_pixel(400, 200, image::Rgb([90, 120, 150]));
        let picture = crate::jpeg::encode(&img, 95, None).unwrap();
        let finish = |long_edge: u32, border: Border| Finish {
            orientation: Orientation::default(),
            crop: None,
            border,
            options: ExportOptions { resize: true, long_edge, ..Default::default() },
        };
        let size = |bytes: Vec<u8>| {
            let i = crate::jpeg::decode(&bytes).unwrap();
            (i.width(), i.height())
        };
        assert_eq!(size(finish(100, Border::default()).apply(picture.clone()).unwrap()), (100, 50));
        assert_eq!(size(finish(1000, Border::default()).apply(picture.clone()).unwrap()), (400, 200));
        // The border is measured on the resized picture: 10% of 50 = 5 px a side.
        let bordered = Border { on: true, percent: 10.0, ..Default::default() };
        assert_eq!(size(finish(100, bordered).apply(picture).unwrap()), (110, 60));
    }
}
