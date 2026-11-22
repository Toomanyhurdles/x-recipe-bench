//! What the app keeps about each photo: stars, crop, and how it is turned;
//! and what it reads from each photo for finding and sorting them.
//!
//! Stars, crop and turning are written to one file in the photo's own
//! folder, `X Recipe Bench.json`, so they come back when the folder is
//! opened again and travel with the folder. The RAF files are never changed.
//! If the folder cannot be written to, the same notes are kept in the
//! app's settings instead.

use crate::orientation::Orientation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const FOLDER_FILE: &str = "X Recipe Bench.json";

/// A crop of the upright photo (after turning): a rectangle, in fractions
/// of the photo's width and height, on the photo tilted by `angle` degrees
/// about its centre (to level a horizon).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub angle: f32,
}

impl Default for Crop {
    fn default() -> Self {
        Crop { x: 0.0, y: 0.0, w: 1.0, h: 1.0, angle: 0.0 }
    }
}

/// The most the photo can be tilted, in degrees either way.
pub const MAX_TILT: f32 = 15.0;

impl Crop {
    /// True if this leaves the photo as it is.
    pub fn is_none(&self) -> bool {
        self.angle.abs() < 0.005 && self.x <= 0.0005 && self.y <= 0.0005 && self.w >= 0.999 && self.h >= 0.999
    }

    /// Where a point of the crop's frame falls on the untilted photo, both
    /// in fractions of width and height. `aspect` is the photo's width
    /// divided by its height (tilting happens in pixels, not fractions).
    pub fn source(&self, u: f32, v: f32, aspect: f32) -> (f32, f32) {
        let (px, py) = ((u - 0.5) * aspect, v - 0.5);
        let (sin, cos) = self.angle.to_radians().sin_cos();
        let (sx, sy) = (cos * px + sin * py, -sin * px + cos * py);
        (sx / aspect + 0.5, sy + 0.5)
    }

    /// The crop's four corners on the untilted photo: top left, top right,
    /// bottom right, bottom left.
    pub fn corners(&self, aspect: f32) -> [(f32, f32); 4] {
        let (x0, y0, x1, y1) = (self.x, self.y, self.x + self.w, self.y + self.h);
        [self.source(x0, y0, aspect), self.source(x1, y0, aspect), self.source(x1, y1, aspect), self.source(x0, y1, aspect)]
    }

    /// True if the whole crop lies on the photo (a tilt cuts the corners off).
    pub fn fits(&self, aspect: f32) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && self.corners(aspect).iter().all(|(u, v)| (-0.0005..=1.0005).contains(u) && (-0.0005..=1.0005).contains(v))
    }

    /// The same crop made smaller about its centre until it lies on the photo.
    pub fn fitted(mut self, aspect: f32) -> Crop {
        if self.fits(aspect) {
            return self;
        }
        let (cx, cy) = (self.x + self.w / 2.0, self.y + self.h / 2.0);
        let centre = self.source(cx, cy, aspect);
        // A centre that has left the photo cannot be shrunk onto it.
        let (cx, cy) = if (0.0..=1.0).contains(&centre.0) && (0.0..=1.0).contains(&centre.1) { (cx, cy) } else { (0.5, 0.5) };
        let (w, h) = (self.w, self.h);
        let scaled = |k: f32| Crop { x: cx - w * k / 2.0, y: cy - h * k / 2.0, w: w * k, h: h * k, angle: self.angle };
        let (mut low, mut high) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = (low + high) / 2.0;
            if scaled(mid).fits(aspect) {
                low = mid;
            } else {
                high = mid;
            }
        }
        self = scaled(low.max(0.02));
        self
    }

    /// The largest centred crop of this shape (width ÷ height, in pixels)
    /// that lies on the photo at this tilt.
    pub fn largest(ratio: f32, angle: f32, aspect: f32) -> Crop {
        // As fractions: w·aspect ÷ h = ratio.
        let (w, h) = if ratio >= aspect { (1.0, aspect / ratio) } else { (ratio / aspect, 1.0) };
        Crop { x: (1.0 - w) / 2.0, y: (1.0 - h) / 2.0, w, h, angle }.fitted(aspect)
    }

    /// Width ÷ height of the cropped picture, in pixels.
    pub fn ratio(&self, aspect: f32) -> f32 {
        self.w * aspect / self.h
    }

    /// The cropped picture. Without a tilt the pixels are copied as they
    /// are; with one, each pixel is worked out from its four neighbours.
    pub fn apply(&self, image: image::RgbImage) -> image::RgbImage {
        if self.is_none() {
            return image;
        }
        let (iw, ih) = (image.width() as f32, image.height() as f32);
        let aspect = iw / ih;
        let crop = self.fitted(aspect);
        let (ow, oh) = (((crop.w * iw).round() as u32).max(1), ((crop.h * ih).round() as u32).max(1));
        if crop.angle.abs() < 0.005 {
            let (x, y) = ((crop.x * iw).round() as u32, (crop.y * ih).round() as u32);
            let (x, y) = (x.min(image.width() - 1), y.min(image.height() - 1));
            return image::imageops::crop_imm(&image, x, y, ow.min(image.width() - x), oh.min(image.height() - y)).to_image();
        }
        let mut out = image::RgbImage::new(ow, oh);
        for (ox, oy, pixel) in out.enumerate_pixels_mut() {
            let (u, v) = (crop.x + (ox as f32 + 0.5) / iw, crop.y + (oy as f32 + 0.5) / ih);
            let (su, sv) = crop.source(u, v, aspect);
            // Position among the source pixels' centres.
            let (fx, fy) = ((su * iw - 0.5).clamp(0.0, iw - 1.0), (sv * ih - 0.5).clamp(0.0, ih - 1.0));
            let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
            let (x1, y1) = ((x0 + 1).min(image.width() - 1), (y0 + 1).min(image.height() - 1));
            let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
            let (a, b, c, d) = (image.get_pixel(x0, y0), image.get_pixel(x1, y0), image.get_pixel(x0, y1), image.get_pixel(x1, y1));
            for channel in 0..3 {
                let top = a[channel] as f32 * (1.0 - tx) + b[channel] as f32 * tx;
                let bottom = c[channel] as f32 * (1.0 - tx) + d[channel] as f32 * tx;
                pixel[channel] = (top * (1.0 - ty) + bottom * ty).round() as u8;
            }
        }
        out
    }
}

/// The app's notes on one photo.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PhotoNote {
    /// 0 for none, otherwise 1 to 5.
    #[serde(skip_serializing_if = "is_zero")]
    pub stars: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<Crop>,
    #[serde(skip_serializing_if = "is_upright")]
    pub orientation: Orientation,
}

fn is_zero(stars: &u8) -> bool {
    *stars == 0
}

fn is_upright(orientation: &Orientation) -> bool {
    orientation.is_identity()
}

impl PhotoNote {
    pub fn is_empty(&self) -> bool {
        self.stars == 0 && self.crop.is_none() && self.orientation.is_identity()
    }
}

/// The contents of a folder's `X Recipe Bench.json`: notes by file name.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FolderNotes {
    #[serde(default)]
    pub photos: HashMap<String, PhotoNote>,
}

impl FolderNotes {
    pub fn load(folder: &Path) -> FolderNotes {
        std::fs::read_to_string(folder.join(FOLDER_FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes the file. Photos with nothing noted are left out, and a
    /// folder with no notes at all gets no file.
    pub fn save(&self, folder: &Path) -> Result<(), String> {
        let path = folder.join(FOLDER_FILE);
        let kept: std::collections::BTreeMap<&String, &PhotoNote> = self.photos.iter().filter(|(_, n)| !n.is_empty()).collect();
        if kept.is_empty() {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("Could not remove {}: {e}", path.display())),
                _ => Ok(()),
            };
        }
        let text = serde_json::to_string_pretty(&serde_json::json!({ "photos": kept })).map_err(|e| e.to_string())?;
        let tmp = folder.join(format!("{FOLDER_FILE}.tmp"));
        std::fs::write(&tmp, text)
            .and_then(|_| std::fs::rename(&tmp, &path))
            .map_err(|e| format!("Could not write {}: {e}", path.display()))
    }
}

/// What is read from a photo for the Browse tab's filters and sorting.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    pub model: String,
    pub lens: String,
    pub f_number: Option<f64>,
    /// Shutter time in seconds.
    pub shutter: Option<f64>,
    pub shutter_label: String,
    pub iso: Option<u32>,
    pub date: String,
    /// The camera's count of shutter actuations when the photo was taken.
    pub shutter_count: Option<u32>,
}

/// Fujifilm's maker-note tag for the image count.
const IMAGE_COUNT_TAG: u16 = 0x1438;

impl Facts {
    pub fn from_exif(exif: &xrp_camera::exif::Exif) -> Facts {
        let shutter = exif.exposure_time.filter(|(_, d)| *d != 0).map(|(n, d)| n as f64 / d as f64);
        let shutter_label = match exif.exposure_time {
            Some((n, d)) if d != 0 && n != 0 && n < d => format!("1/{} s", (d as f64 / n as f64).round()),
            Some((n, d)) if d != 0 => format!("{} s", (n as f64 / d as f64 * 10.0).round() / 10.0),
            _ => String::new(),
        };
        Facts {
            model: exif.model.clone(),
            lens: exif.lens.clone(),
            f_number: exif.f_number,
            shutter,
            shutter_label,
            iso: exif.iso,
            date: exif.date_time.clone().unwrap_or_default(),
            shutter_count: exif.fuji.get(&IMAGE_COUNT_TAG).and_then(|v| v.first()).filter(|v| **v > 0).map(|v| *v as u32),
        }
    }

    pub fn aperture_label(&self) -> String {
        self.f_number.map(|f| format!("f/{}", (f * 10.0).round() / 10.0)).unwrap_or_default()
    }
}

/// What the thumbnails are ordered by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortBy {
    #[default]
    Name,
    Date,
    Stars,
    Lens,
    Aperture,
    Shutter,
    Iso,
}

impl SortBy {
    pub const ALL: [SortBy; 7] =
        [SortBy::Name, SortBy::Date, SortBy::Stars, SortBy::Lens, SortBy::Aperture, SortBy::Shutter, SortBy::Iso];
    pub fn label(self) -> &'static str {
        match self {
            SortBy::Name => "Name",
            SortBy::Date => "Date taken",
            SortBy::Stars => "Stars",
            SortBy::Lens => "Lens",
            SortBy::Aperture => "Aperture",
            SortBy::Shutter => "Shutter speed",
            SortBy::Iso => "ISO",
        }
    }
}

/// Which photos the Browse tab shows, and in what order. `None` means "any".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    /// Show only photos with exactly this many stars; `Some(0)` is the
    /// photos with none.
    pub stars: Option<u8>,
    pub lens: Option<String>,
    pub aperture: Option<String>,
    pub shutter: Option<String>,
    pub iso: Option<u32>,
    pub sort: SortBy,
    pub descending: bool,
}

impl Filter {
    /// True if any photo could be hidden.
    pub fn is_filtering(&self) -> bool {
        self.stars.is_some() || self.lens.is_some() || self.aperture.is_some() || self.shutter.is_some() || self.iso.is_some()
    }

    pub fn clear(&mut self) {
        *self = Filter { sort: self.sort, descending: self.descending, ..Default::default() };
    }

    /// True if a photo with these stars and facts is shown. A photo whose
    /// facts have not been read yet passes only if no fact is asked for.
    pub fn shows(&self, stars: u8, facts: Option<&Facts>) -> bool {
        if self.stars.is_some_and(|wanted| stars != wanted) {
            return false;
        }
        let needs_facts = self.lens.is_some() || self.aperture.is_some() || self.shutter.is_some() || self.iso.is_some();
        let Some(facts) = facts else { return !needs_facts };
        self.lens.as_ref().is_none_or(|l| &facts.lens == l)
            && self.aperture.as_ref().is_none_or(|a| &facts.aperture_label() == a)
            && self.shutter.as_ref().is_none_or(|s| &facts.shutter_label == s)
            && self.iso.is_none_or(|i| facts.iso == Some(i))
    }

    /// The photos to show, in order.
    pub fn apply(&self, files: &[PathBuf], stars: impl Fn(&Path) -> u8, facts: &HashMap<PathBuf, Facts>) -> Vec<PathBuf> {
        let mut shown: Vec<&PathBuf> = files.iter().filter(|f| self.shows(stars(f), facts.get(*f))).collect();
        let number = |v: Option<f64>| v.unwrap_or(f64::MAX);
        let fact = |f: &PathBuf| facts.get(f);
        match self.sort {
            SortBy::Name => {}
            SortBy::Date => shown.sort_by_key(|f| fact(f).map(|x| x.date.clone()).unwrap_or_default()),
            SortBy::Stars => shown.sort_by_key(|f| stars(f)),
            SortBy::Lens => shown.sort_by_key(|f| fact(f).map(|x| x.lens.clone()).unwrap_or_default()),
            SortBy::Aperture => shown.sort_by(|a, b| {
                number(fact(a).and_then(|x| x.f_number)).total_cmp(&number(fact(b).and_then(|x| x.f_number)))
            }),
            SortBy::Shutter => shown.sort_by(|a, b| {
                number(fact(a).and_then(|x| x.shutter)).total_cmp(&number(fact(b).and_then(|x| x.shutter)))
            }),
            SortBy::Iso => shown.sort_by_key(|f| fact(f).and_then(|x| x.iso).unwrap_or(u32::MAX)),
        }
        if self.descending {
            shown.reverse();
        }
        shown.into_iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_crop_copies_the_pixels_and_a_tilt_stays_on_the_photo() {
        let mut photo = image::RgbImage::new(200, 100);
        for (x, _, p) in photo.enumerate_pixels_mut() {
            *p = image::Rgb([x as u8, 0, 0]);
        }
        let right_half = Crop { x: 0.5, y: 0.0, w: 0.5, h: 1.0, angle: 0.0 };
        let out = right_half.apply(photo.clone());
        assert_eq!((out.width(), out.height()), (100, 100));
        assert_eq!(out.get_pixel(0, 0)[0], 100);
        assert!(Crop::default().is_none());
        assert_eq!(Crop::default().apply(photo.clone()), photo);

        // Tilted 10°: the full frame no longer fits; the largest 2:1 crop does.
        let full = Crop { angle: 10.0, ..Default::default() };
        assert!(!full.fits(2.0));
        let largest = Crop::largest(2.0, 10.0, 2.0);
        assert!(largest.fits(2.0) && largest.w < 0.9 && largest.w > 0.5);
        assert!((largest.ratio(2.0) - 2.0).abs() < 0.01);
        let out = largest.apply(photo);
        assert!((out.width() as f32 / out.height() as f32 - 2.0).abs() < 0.05);
        // A square from a 2:1 photo, untilted, is the full height.
        let square = Crop::largest(1.0, 0.0, 2.0);
        assert!((square.h - 1.0).abs() < 0.001 && (square.w - 0.5).abs() < 0.001);
    }

    #[test]
    fn notes_are_written_to_the_folder_and_read_back() {
        let dir = std::env::temp_dir().join(format!("xrb-notes-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut notes = FolderNotes::default();
        notes.photos.insert(
            "DSCF0001.RAF".into(),
            PhotoNote {
                stars: 4,
                crop: Some(Crop { x: 0.1, y: 0.2, w: 0.5, h: 0.5, angle: -2.5 }),
                orientation: Orientation::default().rotated_cw(),
            },
        );
        notes.photos.insert("DSCF0002.RAF".into(), PhotoNote::default());
        notes.save(&dir).unwrap();
        let back = FolderNotes::load(&dir);
        assert_eq!(back.photos.len(), 1, "photos with nothing noted are not listed");
        assert_eq!(back.photos["DSCF0001.RAF"], notes.photos["DSCF0001.RAF"]);
        // Nothing left to note: the file goes away.
        FolderNotes::default().save(&dir).unwrap();
        assert!(!dir.join(FOLDER_FILE).exists());
        assert_eq!(FolderNotes::load(&dir), FolderNotes::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn photos_are_filtered_and_sorted() {
        let files: Vec<PathBuf> = ["a.RAF", "b.RAF", "c.RAF", "d.RAF"].iter().map(PathBuf::from).collect();
        let fact = |lens: &str, f: f64, shutter: (u32, u32), iso: u32| {
            Facts::from_exif(&xrp_camera::exif::Exif {
                lens: lens.into(),
                f_number: Some(f),
                exposure_time: Some(shutter),
                iso: Some(iso),
                ..Default::default()
            })
        };
        let mut facts = HashMap::new();
        facts.insert(files[0].clone(), fact("XF23mm", 2.8, (1, 250), 640));
        facts.insert(files[1].clone(), fact("XF35mm", 1.4, (1, 60), 3200));
        facts.insert(files[2].clone(), fact("XF23mm", 8.0, (2, 1), 160));
        // d.RAF has not been read yet.
        let stars = |p: &Path| match p.to_str().unwrap() {
            "a.RAF" => 3,
            "b.RAF" => 5,
            _ => 0,
        };
        let names = |list: Vec<PathBuf>| list.iter().map(|p| p.to_str().unwrap()[..1].to_string()).collect::<String>();

        let mut filter = Filter::default();
        assert_eq!(names(filter.apply(&files, stars, &facts)), "abcd");
        filter.stars = Some(3);
        assert_eq!(names(filter.apply(&files, stars, &facts)), "a", "exactly three, not three and up");
        filter.stars = Some(5);
        assert_eq!(names(filter.apply(&files, stars, &facts)), "b");
        filter.stars = Some(0);
        assert_eq!(names(filter.apply(&files, stars, &facts)), "cd", "photos with no stars");
        filter.clear();
        filter.lens = Some("XF23mm".into());
        assert_eq!(names(filter.apply(&files, stars, &facts)), "ac");
        filter.aperture = Some("f/8".into());
        assert_eq!(names(filter.apply(&files, stars, &facts)), "c");
        filter.clear();
        filter.shutter = Some("1/60 s".into());
        assert_eq!(names(filter.apply(&files, stars, &facts)), "b");

        filter.clear();
        filter.sort = SortBy::Aperture;
        assert_eq!(names(filter.apply(&files, stars, &facts)), "bacd");
        filter.sort = SortBy::Shutter;
        assert_eq!(names(filter.apply(&files, stars, &facts)), "abcd");
        filter.sort = SortBy::Iso;
        filter.descending = true;
        assert_eq!(names(filter.apply(&files, stars, &facts)), "dbac");
        assert_eq!(facts[&files[2]].shutter_label, "2 s");
    }
}
