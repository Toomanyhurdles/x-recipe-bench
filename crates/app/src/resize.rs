//! Making pictures smaller.
//!
//! The resampling is craft-develop's (craft-libs): separable filters with
//! precomputed weights, spread over the processor's cores. These helpers
//! convert to and from the `image` crate's `RgbImage`.

use craft_develop::raster::resample::{self, Filter};
use craft_develop::raster::Image;
use image::RgbImage;

/// How carefully to resample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// Sharp and clean, for exported pictures (Lanczos).
    Best,
    /// Quick, for thumbnails and samples (an average of the pixels covered).
    Fast,
}

impl Quality {
    fn filter(self) -> Filter {
        match self {
            Quality::Best => Filter::Lanczos3,
            Quality::Fast => Filter::Box,
        }
    }
}

/// `img` resampled to exactly `width` × `height`.
pub fn resize(img: &RgbImage, width: u32, height: u32, quality: Quality) -> RgbImage {
    from_float(&resample::resize(&to_float(img), width.max(1) as usize, height.max(1) as usize, quality.filter()))
}

/// `img` made to fit inside `max_side` × `max_side`, keeping its shape. Never enlarges.
pub fn fit(img: &RgbImage, max_side: u32, quality: Quality) -> RgbImage {
    if img.width().max(img.height()) <= max_side {
        return img.clone();
    }
    let side = max_side.max(1) as usize;
    from_float(&resample::fit(&to_float(img), side, side, quality.filter()))
}

fn to_float(img: &RgbImage) -> Image<[f32; 3]> {
    let data = img.pixels().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
    Image { width: img.width() as usize, height: img.height() as usize, data }
}

fn from_float(img: &Image<[f32; 3]>) -> RgbImage {
    let byte = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    let data = img.data.iter().flat_map(|p| [byte(p[0]), byte(p[1]), byte(p[2])]).collect();
    RgbImage::from_raw(img.width as u32, img.height as u32, data).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_colours_come_out_right() {
        let img = RgbImage::from_pixel(400, 200, image::Rgb([90, 120, 150]));
        let small = resize(&img, 100, 50, Quality::Best);
        assert_eq!(small.dimensions(), (100, 50));
        assert_eq!(small.get_pixel(50, 25).0, [90, 120, 150]);
        assert_eq!(fit(&img, 100, Quality::Fast).dimensions(), (100, 50));
        // Already small enough: unchanged.
        assert_eq!(fit(&img, 1000, Quality::Fast).dimensions(), (400, 200));
    }
}
