//! The optional white border around a photo.
//!
//! The border is added around the picture (nothing is cropped), and its
//! width is a percentage of the picture's shorter side, so it looks the
//! same on a small preview and a full-size export. It is a setting of the
//! app, not part of a recipe.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Border {
    pub on: bool,
    /// Width on each side, as a percentage of the picture's shorter side.
    pub percent: f32,
    /// Extra width on the bottom edge only, same unit (a "print" look).
    pub bottom_extra: f32,
    /// Red, green, blue.
    pub color: [u8; 3],
}

impl Default for Border {
    fn default() -> Self {
        Border { on: false, percent: 5.0, bottom_extra: 0.0, color: [255, 255, 255] }
    }
}

impl Border {
    /// The widest border offered.
    pub const MAX_PERCENT: f32 = 25.0;

    /// Width as a fraction of the shorter side; zero when the border is off.
    pub fn fraction(self) -> f32 {
        if self.on {
            self.percent.clamp(0.0, Self::MAX_PERCENT) / 100.0
        } else {
            0.0
        }
    }

    pub fn is_none(self) -> bool {
        self.fraction() <= 0.0
    }

    /// Border width in pixels for a picture of this size.
    pub fn pixels(self, width: u32, height: u32) -> u32 {
        (width.min(height) as f32 * self.fraction()).round() as u32
    }

    /// The extra on the bottom edge, as a fraction of the shorter side.
    pub fn bottom_fraction(self) -> f32 {
        if self.is_none() {
            0.0
        } else {
            self.bottom_extra.clamp(0.0, Self::MAX_PERCENT) / 100.0
        }
    }

    /// The picture with the border added around it.
    pub fn add(self, image: image::RgbImage) -> image::RgbImage {
        let b = self.pixels(image.width(), image.height());
        if b == 0 {
            return image;
        }
        let extra = (image.width().min(image.height()) as f32 * self.bottom_fraction()).round() as u32;
        let mut out =
            image::RgbImage::from_pixel(image.width() + 2 * b, image.height() + 2 * b + extra, image::Rgb(self.color));
        image::imageops::replace(&mut out, &image, b as i64, b as i64);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn border_is_measured_from_the_shorter_side_and_added_around() {
        let photo = image::RgbImage::from_pixel(200, 100, image::Rgb([10, 20, 30]));
        let out = Border { on: true, percent: 10.0, ..Default::default() }.add(photo);
        assert_eq!((out.width(), out.height()), (220, 120));
        assert_eq!(out.get_pixel(0, 0).0, [255, 255, 255]);
        assert_eq!(out.get_pixel(9, 60).0, [255, 255, 255]);
        assert_eq!(out.get_pixel(10, 10).0, [10, 20, 30]);
        assert_eq!(out.get_pixel(209, 109).0, [10, 20, 30]);
        assert_eq!(out.get_pixel(210, 60).0, [255, 255, 255]);
    }

    #[test]
    fn off_or_zero_changes_nothing() {
        let photo = image::RgbImage::from_pixel(8, 6, image::Rgb([1, 2, 3]));
        assert_eq!(Border { on: false, percent: 10.0, ..Default::default() }.add(photo.clone()), photo);
        assert_eq!(Border { on: true, percent: 0.0, ..Default::default() }.add(photo.clone()), photo);
        assert!(Border::default().is_none());
        // Out-of-range numbers from a hand-edited settings file are limited.
        assert_eq!(Border { on: true, percent: 900.0, ..Default::default() }.pixels(100, 100), 25);
    }

    #[test]
    fn colour_and_a_wider_bottom_edge() {
        let photo = image::RgbImage::from_pixel(200, 100, image::Rgb([10, 20, 30]));
        let out = Border { on: true, percent: 10.0, bottom_extra: 20.0, color: [0, 0, 0] }.add(photo);
        assert_eq!((out.width(), out.height()), (220, 140));
        assert_eq!(out.get_pixel(0, 0).0, [0, 0, 0]);
        assert_eq!(out.get_pixel(100, 109).0, [10, 20, 30]);
        assert_eq!(out.get_pixel(100, 139).0, [0, 0, 0]);
    }
}
