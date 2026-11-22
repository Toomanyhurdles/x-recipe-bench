//! How a photo is turned: quarter turns and mirror flips.
//!
//! An orientation means "mirror left-to-right first if `flip` is set, then
//! turn clockwise by `turns` quarter turns". Every combination of turns and
//! flips, in any order, comes out as one of these eight.

use eframe::egui::{pos2, Pos2, Vec2};
use image::DynamicImage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Orientation {
    /// Clockwise quarter turns, 0..=3.
    pub turns: u8,
    /// Mirrored left-to-right before turning.
    pub flip: bool,
}

impl Orientation {
    pub fn is_identity(self) -> bool {
        self == Orientation::default()
    }

    pub fn rotated_cw(self) -> Self {
        Orientation { turns: (self.turns + 1) % 4, ..self }
    }

    pub fn rotated_ccw(self) -> Self {
        Orientation { turns: (self.turns + 3) % 4, ..self }
    }

    /// Mirrors the picture as it is currently shown, left to right.
    pub fn flipped_horizontally(self) -> Self {
        // Mirroring after a turn equals the opposite turn after mirroring.
        Orientation { turns: (4 - self.turns) % 4, flip: !self.flip }
    }

    /// Mirrors the picture as it is currently shown, top to bottom.
    pub fn flipped_vertically(self) -> Self {
        let mut o = self.flipped_horizontally();
        o.turns = (o.turns + 2) % 4;
        o
    }

    /// The orientation a camera recorded in a photo (EXIF values 1..=8).
    pub fn from_exif(value: u16) -> Self {
        let (turns, flip) = match value {
            2 => (0, true),
            3 => (2, false),
            4 => (2, true),
            5 => (3, true),
            6 => (1, false),
            7 => (1, true),
            8 => (3, false),
            _ => (0, false),
        };
        Orientation { turns, flip }
    }

    /// True when width and height change places.
    pub fn swaps_sides(self) -> bool {
        self.turns % 2 == 1
    }

    /// Size of a picture of `size` once shown in this orientation.
    pub fn shown_size(self, size: Vec2) -> Vec2 {
        if self.swaps_sides() {
            Vec2::new(size.y, size.x)
        } else {
            size
        }
    }

    /// Which point of the stored picture belongs at each corner of the shown
    /// picture, in the order top-left, top-right, bottom-right, bottom-left.
    pub fn corners(self) -> [Pos2; 4] {
        let mut c = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)];
        if self.flip {
            c = [c[1], c[0], c[3], c[2]];
        }
        // One clockwise turn moves the stored bottom-left to the shown top-left.
        c.rotate_right(self.turns as usize % 4);
        c
    }

    /// Turns the pixels themselves.
    pub fn apply(self, image: DynamicImage) -> DynamicImage {
        let image = if self.flip { image.fliph() } else { image };
        match self.turns % 4 {
            1 => image.rotate90(),
            2 => image.rotate180(),
            3 => image.rotate270(),
            _ => image,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgb, RgbImage};

    /// A 2×3 picture where every pixel is different.
    fn sample() -> DynamicImage {
        let mut img = RgbImage::new(2, 3);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgb([x as u8 * 100 + 10, y as u8 * 80 + 10, 0]);
        }
        DynamicImage::ImageRgb8(img)
    }

    fn all() -> Vec<Orientation> {
        (0..4).flat_map(|turns| [false, true].map(|flip| Orientation { turns, flip })).collect()
    }

    #[test]
    fn four_turns_and_two_flips_come_back_to_the_start() {
        for o in all() {
            assert_eq!(o.rotated_cw().rotated_cw().rotated_cw().rotated_cw(), o);
            assert_eq!(o.rotated_cw().rotated_ccw(), o);
            assert_eq!(o.flipped_horizontally().flipped_horizontally(), o);
            assert_eq!(o.flipped_vertically().flipped_vertically(), o);
        }
    }

    #[test]
    fn the_buttons_do_to_the_pixels_what_their_names_say() {
        for o in all() {
            let shown = o.apply(sample());
            assert_eq!(o.rotated_cw().apply(sample()).to_rgb8(), shown.rotate90().to_rgb8());
            assert_eq!(o.rotated_ccw().apply(sample()).to_rgb8(), shown.rotate270().to_rgb8());
            assert_eq!(o.flipped_horizontally().apply(sample()).to_rgb8(), shown.fliph().to_rgb8());
            assert_eq!(o.flipped_vertically().apply(sample()).to_rgb8(), shown.flipv().to_rgb8());
        }
    }

    #[test]
    fn the_drawn_corners_match_the_turned_pixels() {
        let original = sample();
        let (w, h) = original.dimensions();
        for o in all() {
            let turned = o.apply(sample());
            let (tw, th) = turned.dimensions();
            assert_eq!(o.swaps_sides(), (tw, th) == (h, w) && w != h);
            // Each corner of the turned picture is the stored pixel `corners` names.
            let shown = [(0, 0), (tw - 1, 0), (tw - 1, th - 1), (0, th - 1)];
            for (corner, uv) in shown.iter().zip(o.corners()) {
                let source = ((uv.x * (w - 1) as f32) as u32, (uv.y * (h - 1) as f32) as u32);
                assert_eq!(turned.get_pixel(corner.0, corner.1), original.get_pixel(source.0, source.1), "{o:?}");
            }
        }
    }

    #[test]
    fn camera_orientations_are_understood() {
        // 6: the camera was held so the picture needs a clockwise quarter turn.
        assert_eq!(Orientation::from_exif(6), Orientation { turns: 1, flip: false });
        assert_eq!(Orientation::from_exif(8), Orientation { turns: 3, flip: false });
        assert_eq!(Orientation::from_exif(3), Orientation { turns: 2, flip: false });
        assert!(Orientation::from_exif(1).is_identity() && Orientation::from_exif(99).is_identity());
        // 4 is a top-to-bottom mirror; 5 and 7 are the two diagonal mirrors.
        assert_eq!(Orientation::from_exif(4).apply(sample()).to_rgb8(), sample().flipv().to_rgb8());
        assert_eq!(Orientation::from_exif(5).apply(sample()).to_rgb8(), sample().fliph().rotate270().to_rgb8());
        assert_eq!(Orientation::from_exif(7).apply(sample()).to_rgb8(), sample().fliph().rotate90().to_rgb8());
    }
}
