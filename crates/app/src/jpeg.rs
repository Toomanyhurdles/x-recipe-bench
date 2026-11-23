//! Reading and writing JPEG pictures.
//!
//! The codec is `craft-jpeg` from craft-libs. These helpers turn its output
//! into the `image` crate's `RgbImage`, which the rest of the app uses for
//! turning, cropping, resizing and borders.

use image::{DynamicImage, RgbImage};

/// A JPEG decoded at full size.
pub fn decode(bytes: &[u8]) -> Result<DynamicImage, String> {
    let decoded = craft_jpeg::decode(bytes, &craft_jpeg::Limits::default(), craft_jpeg::adobe_transform(bytes))
        .map_err(|e| e.to_string())?;
    to_image(decoded)
}

/// A JPEG decoded at a size whose long side is at least `min_side`, using the
/// fast reduced decode (1/2, 1/4 or 1/8) when the picture is big enough.
/// Much quicker than a full decode for thumbnails.
pub fn decode_at_least(bytes: &[u8], min_side: u32) -> Result<DynamicImage, String> {
    let (w, h) = size(bytes).unwrap_or((0, 0));
    let long = w.max(h);
    // The largest shrink (1..=3, i.e. 1/2..1/8) that still leaves `min_side` pixels.
    let shrink = (1..=3u8).rev().find(|s| long >> s >= min_side).unwrap_or(0);
    if shrink == 0 {
        return decode(bytes);
    }
    let decoded = craft_jpeg::decode_reduced(bytes, &craft_jpeg::Limits::default(), craft_jpeg::adobe_transform(bytes), shrink)
        .map_err(|e| e.to_string())?;
    to_image(decoded)
}

/// The picture's width and height, read from its header.
pub fn size(bytes: &[u8]) -> Option<(u32, u32)> {
    let header = craft_metadata::inspect_jpeg(bytes)?;
    Some((header.width, header.height))
}

fn to_image(decoded: craft_jpeg::Decoded) -> Result<DynamicImage, String> {
    let craft_jpeg::Samples::U8(samples) = decoded.samples else {
        return Err("12-bit JPEG pictures are not supported".into());
    };
    let rgb = match decoded.components {
        3 => samples,
        // Grey and CMYK pictures go through RGBA, then the alpha is dropped.
        n => craft_jpeg::to_rgba8(&samples, n)
            .map_err(|_| format!("pictures with {n} colour channels are not supported"))?
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
    };
    RgbImage::from_raw(decoded.width, decoded.height, rgb)
        .map(DynamicImage::ImageRgb8)
        .ok_or_else(|| "the decoded picture has the wrong size".to_string())
}

/// Saves `image` as a JPEG at `quality` (1–100). When `details_from` is the
/// camera's JPEG, its EXIF block goes along, marked upright because the pixels
/// already are.
pub fn encode(image: &RgbImage, quality: u8, details_from: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let segments: Vec<(u8, Vec<u8>)> = details_from.and_then(xrp_camera::exif::upright_segment).into_iter().collect();
    craft_jpeg::encode_baseline(
        image.as_raw(),
        image.width() as usize,
        image.height() as usize,
        3,
        quality.clamp(1, 100),
        // The same colour detail as the camera's own JPEGs.
        craft_jpeg::ChromaSubsampling::S422,
        &segments,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_survives_saving_and_reading() {
        let img = RgbImage::from_fn(64, 32, |x, _| image::Rgb([(x * 4) as u8, 100, 200]));
        let jpeg = encode(&img, 95, None).unwrap();
        assert_eq!(size(&jpeg), Some((64, 32)));
        let back = decode(&jpeg).unwrap().to_rgb8();
        assert_eq!(back.dimensions(), (64, 32));
        let p = back.get_pixel(40, 10);
        assert!((p[0] as i32 - 160).abs() < 12 && (p[2] as i32 - 200).abs() < 12, "{p:?}");
        // A quick small decode for thumbnails: 1/4 still covers 16 pixels.
        assert_eq!(decode_at_least(&jpeg, 16).unwrap().width(), 16);
        assert!(decode(b"not a picture").is_err());
    }

    #[test]
    fn the_cameras_details_go_along() {
        let img = RgbImage::from_pixel(16, 16, image::Rgb([1, 2, 3]));
        let camera = xrp_camera::exif::test_jpeg(6, &[(0x1401, 3, vec![0x600])]);
        let jpeg = encode(&img, 90, Some(&camera)).unwrap();
        let exif = xrp_camera::exif::parse_jpeg(&jpeg).unwrap();
        assert_eq!((exif.model.as_str(), exif.orientation), ("X-T4", 1));
        assert_eq!(exif.fuji.get(&0x1401), Some(&vec![0x600]));
    }
}

