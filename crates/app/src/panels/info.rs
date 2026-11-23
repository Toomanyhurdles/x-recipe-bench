//! Info: what the selected photo says about itself. Read from the file, so
//! it works without the camera.

use super::hint;
use crate::state::AppState;
use eframe::egui::{self, RichText, Ui};

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let (Some(raf), Some(info)) = (&state.selected, &state.info) else {
        hint(ui, "Select a photo to see its details.");
        return;
    };
    let raf_path = raf.clone();
    let stars = state.stars_of(raf);
    let mut new_stars = None;

    egui::ScrollArea::vertical().id_salt("info_scroll").auto_shrink(false).show(ui, |ui| {
        let name = raf.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        ui.strong(&name);
        if let Some(stars) = super::crop::stars_row(ui, stars) {
            new_stars = Some(stars);
        }

        let mut shooting: Vec<(&str, String)> = Vec::new();
        if let Some(exif) = &info.exif {
            let camera = [exif.make.as_str(), exif.model.as_str()].join(" ").trim().to_string();
            if !camera.is_empty() {
                shooting.push(("Camera", camera));
            }
            if !exif.lens.is_empty() {
                shooting.push(("Lens", exif.lens.clone()));
            }
            if let Some(mm) = exif.focal_length {
                shooting.push(("Focal length", format!("{} mm", trim_number(mm))));
            }
            if let Some(seconds) = exif.exposure_time {
                shooting.push(("Shutter", xrp_camera::exif::shutter_label(seconds)));
            }
            if let Some(f) = exif.f_number {
                shooting.push(("Aperture", format!("f/{}", trim_number(f))));
            }
            if let Some(iso) = exif.iso {
                shooting.push(("ISO", iso.to_string()));
            }
            if let Some(count) = crate::photo::Facts::from_exif(exif).shutter_count {
                shooting.push(("Shutter count", count.to_string()));
            }
            if let Some(ev) = exif.exposure_bias {
                let thirds = (ev * 3.0).round() as i32;
                shooting.push(("Exposure comp.", xrp_recipes::settings::exposure_label(thirds)));
            }
            if let Some(when) = &exif.date_time {
                shooting.push(("Taken", friendly_date(when)));
            }
        }

        section(ui, "Shooting");
        if shooting.is_empty() {
            hint(ui, "This file has no readable shooting details.");
        } else {
            grid(ui, "info_shooting", shooting.iter().map(|(a, b)| (*a, b.as_str())));
        }

        section(ui, "File");
        let folder = raf.parent().map(|p| p.display().to_string()).unwrap_or_default();
        let size = format!("{:.1} MB", info.file_size as f64 / 1_048_576.0);
        grid(ui, "info_file", [("Size", size.as_str()), ("Folder", folder.as_str())].into_iter());

        section(ui, "Look as shot");
        match &info.look {
            Some(look) => {
                let lines = look.summary();
                grid(ui, "info_look", lines.iter().map(|(a, b)| (*a, b.as_str())));
            }
            None => hint(ui, "The camera's look settings could not be read from this file."),
        }
    });
    if let Some(stars) = new_stars {
        state.set_stars(&raf_path, stars);
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).strong().weak());
}

fn grid<'a>(ui: &mut Ui, id: &str, rows: impl Iterator<Item = (&'a str, &'a str)>) {
    egui::Grid::new(id).num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
        for (label, value) in rows {
            ui.label(RichText::new(label).weak());
            ui.add(egui::Label::new(value).wrap());
            ui.end_row();
        }
    });
}

/// 23.0 → "23", 2.8 → "2.8".
fn trim_number(v: f64) -> String {
    let text = format!("{v:.1}");
    text.strip_suffix(".0").map(str::to_string).unwrap_or(text)
}

/// "2026:11:17 09:12:44" → "2026-11-17 09:12".
fn friendly_date(exif: &str) -> String {
    match exif.split_once(' ') {
        Some((date, time)) => format!("{} {}", date.replace(':', "-"), time.get(..5).unwrap_or(time)),
        None => exif.to_string(),
    }
}
