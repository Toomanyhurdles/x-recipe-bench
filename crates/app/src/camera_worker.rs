//! Background thread that owns the camera, so the window never freezes
//! while the camera is busy.

use crate::events::{AppEvent, EventSender, PreviewKey};
use crate::export::Finish;
use crate::orientation::Orientation;
use crate::state::CameraStatus;
use eframe::egui::ColorImage;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;
use xrp_camera::profile::Profile;
use xrp_camera::transport::{Transport, UsbTransport};
use xrp_camera::{busy, Camera, Error, RenderSize};
use crate::slots::{SlotInfo, SlotReport};
use xrp_camera::slots;
use xrp_recipes::{apply, library, slot, Recipe, Settings};

/// A real camera on USB, or the simulated one.
type AnyTransport = Box<dyn Transport + Send>;

/// Connects to the camera. With `XRP_SIMULATED_CAMERA` set, a simulated
/// X-T4 with seven custom slots is used instead, for trying the app's
/// camera screens without hardware (it cannot develop real pictures).
fn open_camera() -> Result<Camera<AnyTransport>, Error> {
    if std::env::var_os("XRP_SIMULATED_CAMERA").is_some() {
        return Camera::start(Box::new(xrp_camera::sim::FakeCamera::with_slots()) as AnyTransport);
    }
    Camera::start(Box::new(UsbTransport::open()?) as AnyTransport)
}

/// A recipe to develop with.
#[derive(Clone)]
pub struct RecipeJob {
    pub file_name: String,
    pub name: String,
    pub settings: Settings,
}

pub enum CameraCommand {
    Connect,
    Disconnect,
    /// Stop the desktop's camera service, then connect.
    FreeFromDesktopService,
    /// Develop a reduced-size preview for one of the copies in the Preview
    /// panel (`slot`). `recipe: None` means the original look.
    Preview { slot: usize, raf: PathBuf, recipe: Option<RecipeJob> },
    /// Develop at full size and write the result to `dest`, finished as
    /// `finish` says (turned, resized, bordered).
    Export { raf: PathBuf, recipe: Option<RecipeJob>, dest: PathBuf, finish: Finish },
    /// Drop every export that has not started yet.
    CancelExports,
    /// Develop `raf` with a recipe at preview size and save it, small, as
    /// the recipe's sample picture. Done only when nothing else is waiting.
    Sample { raf: PathBuf, job: RecipeJob, dest: PathBuf, orientation: Orientation },
    /// Read every custom slot (C1, C2, …).
    ReadSlots,
    /// Store a recipe and a name in custom slot `number`. What the slot
    /// held is first saved as a recipe file in `backup_folder`.
    WriteSlot { number: u16, name: String, job: RecipeJob, backup_folder: PathBuf },
}

pub fn spawn(events: EventSender) -> Sender<CameraCommand> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("camera".into())
        .spawn(move || {
            Worker { events, camera: None, loaded: None, profile_changed: false }.run(rx)
        })
        .expect("could not start the camera thread");
    tx
}

/// The RAF currently held by the camera.
struct Loaded {
    raf: PathBuf,
    /// Its profile as first read: the settings it was shot with.
    as_shot: Profile,
}

struct Worker {
    events: EventSender,
    camera: Option<Camera<AnyTransport>>,
    loaded: Option<Loaded>,
    /// True once a recipe has been written, so "as shot" must be written back.
    profile_changed: bool,
}

/// Removes every preview request that a later request for the same copy
/// replaces. Returns what the removed requests were for.
fn drop_superseded_previews(queue: &mut std::collections::VecDeque<CameraCommand>) -> Vec<PreviewKey> {
    let mut newest: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (index, command) in queue.iter().enumerate() {
        if let CameraCommand::Preview { slot, .. } = command {
            newest.insert(*slot, index);
        }
    }
    let mut skipped = Vec::new();
    let mut index = 0;
    queue.retain(|command| {
        let keep = match command {
            CameraCommand::Preview { slot, raf, recipe } => {
                let keep = newest.get(slot) == Some(&index);
                if !keep {
                    skipped.push(key(raf, recipe));
                }
                keep
            }
            _ => true,
        };
        index += 1;
        keep
    });
    // A request dropped in favour of an identical one is not really skipped.
    let kept: Vec<PreviewKey> = queue
        .iter()
        .filter_map(|c| match c {
            CameraCommand::Preview { raf, recipe, .. } => Some(key(raf, recipe)),
            _ => None,
        })
        .collect();
    skipped.retain(|k| !kept.contains(k));
    skipped
}

fn key(raf: &Path, recipe: &Option<RecipeJob>) -> PreviewKey {
    PreviewKey {
        raf: raf.to_path_buf(),
        recipe: recipe.as_ref().map(|r| r.file_name.clone()),
        variant: recipe.as_ref().map(|r| r.settings.fingerprint()).unwrap_or(0),
    }
}

impl Worker {
    fn run(mut self, rx: Receiver<CameraCommand>) {
        let mut queue: std::collections::VecDeque<CameraCommand> = std::collections::VecDeque::new();
        loop {
            if queue.is_empty() {
                match rx.recv() {
                    Ok(command) => queue.push_back(command),
                    Err(_) => break,
                }
            }
            while let Ok(command) = rx.try_recv() {
                queue.push_back(command);
            }
            // If previews have piled up for a copy, only its newest matters.
            for skipped in drop_superseded_previews(&mut queue) {
                self.events.send(AppEvent::PreviewSkipped(skipped));
            }
            // A cancel removes the exports waiting behind it.
            if queue.iter().any(|c| matches!(c, CameraCommand::CancelExports)) {
                let before = queue.len();
                queue.retain(|c| !matches!(c, CameraCommand::Export { .. } | CameraCommand::CancelExports));
                // One of the removed entries was the cancel itself.
                self.events.send(AppEvent::ExportsCancelled(before - queue.len() - 1));
            }
            // Sample pictures wait for everything else.
            let next = queue.iter().position(|c| !matches!(c, CameraCommand::Sample { .. })).unwrap_or(0);
            let Some(command) = queue.remove(next) else { continue };
            match command {
                CameraCommand::Connect => self.connect(),
                CameraCommand::Disconnect => self.disconnect(),
                CameraCommand::FreeFromDesktopService => {
                    match busy::release_from_desktop_service() {
                        Ok(()) => self.events.log("Stopped the desktop camera service until next login."),
                        Err(e) => self.events.log(e),
                    }
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    self.connect();
                }
                CameraCommand::Preview { raf, recipe, .. } => {
                    let started = Instant::now();
                    let key = key(&raf, &recipe);
                    let result = self
                        .develop(&raf, recipe.as_ref(), RenderSize::Preview)
                        .and_then(|(bytes, notes)| Ok((decode(&bytes)?, notes)));
                    match result {
                        Ok((image, notes)) => self.events.send(AppEvent::Developed {
                            key,
                            image,
                            seconds: started.elapsed().as_secs_f32(),
                            notes,
                        }),
                        Err(e) => self.fail(key, e, false),
                    }
                }
                CameraCommand::Export { raf, recipe, dest, finish } => {
                    let started = Instant::now();
                    let key = key(&raf, &recipe);
                    let result = self.develop(&raf, recipe.as_ref(), RenderSize::Full).and_then(|(bytes, notes)| {
                        let bytes = finish.apply(bytes)?;
                        std::fs::write(&dest, &bytes)
                            .map_err(|e| Error::Other(format!("Could not save {}: {e}", dest.display())))?;
                        Ok(notes)
                    });
                    match result {
                        Ok(notes) => {
                            for note in notes {
                                self.events.log(format!("Note: {note}"));
                            }
                            self.events.send(AppEvent::Exported {
                                dest,
                                seconds: started.elapsed().as_secs_f32(),
                            })
                        }
                        Err(e) => self.fail(key, e, true),
                    }
                }
                CameraCommand::CancelExports => {}
                CameraCommand::Sample { raf, job, dest, orientation } => {
                    let result = self.develop(&raf, Some(&job), RenderSize::Preview).and_then(|(bytes, _notes)| {
                        save_sample(&bytes, orientation, &dest)
                    });
                    if let Err(error) = &result {
                        self.drop_if_lost(error);
                    }
                    self.events.send(AppEvent::SampleMade {
                        recipe_file: job.file_name,
                        result: result.map_err(|e| e.to_string()),
                    });
                }
                CameraCommand::ReadSlots => {
                    let result = self.read_slots();
                    if let Err(error) = &result {
                        self.drop_if_lost(error);
                    }
                    self.events.send(AppEvent::Slots(result.map_err(|e| e.to_string())));
                }
                CameraCommand::WriteSlot { number, name, job, backup_folder } => {
                    let result = self.upload_to_slot(number, &name, &job, &backup_folder);
                    if let Err(error) = &result {
                        self.drop_if_lost(error);
                    }
                    // Show the slots as they are now, whatever happened.
                    if self.camera.is_some() {
                        if let Ok(slots) = self.read_slots() {
                            self.events.send(AppEvent::Slots(Ok(slots)));
                        }
                    }
                    self.events.send(AppEvent::SlotWritten(result.map_err(|e| e.to_string())));
                }
            }
        }
        self.disconnect();
    }

    fn connect(&mut self) {
        if self.camera.is_some() {
            return;
        }
        self.loaded = None;
        self.events.send(AppEvent::Camera(CameraStatus::Connecting));
        let status = match open_camera() {
            Ok(camera) => {
                let status = CameraStatus::Connected {
                    model: camera.info.model.clone(),
                    firmware: camera.info.firmware.clone(),
                    mode: camera.mode.label(),
                    ready: camera.info.can_convert() && camera.require_conversion_mode().is_ok(),
                };
                self.events.log(format!(
                    "Connected to {} (firmware {}), mode: {}.",
                    camera.info.model,
                    camera.info.firmware,
                    camera.mode.label()
                ));
                self.camera = Some(camera);
                status
            }
            Err(Error::NoCamera) => CameraStatus::NotFound,
            Err(Error::Busy { holder }) => {
                self.events.log(Error::Busy { holder: holder.clone() }.to_string());
                CameraStatus::Busy { holder }
            }
            Err(e) => {
                self.events.log(format!("Could not connect: {e}"));
                CameraStatus::Failed(e.to_string())
            }
        };
        self.events.send(AppEvent::Camera(status));
    }

    fn disconnect(&mut self) {
        if let Some(camera) = self.camera.take() {
            camera.close();
            self.events.log("Disconnected from the camera.");
        }
        self.loaded = None;
        self.events.send(AppEvent::Camera(CameraStatus::Disconnected));
    }

    /// Develops `raf` with `recipe` (or as shot). Returns the image and any notes.
    fn develop(
        &mut self,
        raf: &Path,
        recipe: Option<&RecipeJob>,
        size: RenderSize,
    ) -> Result<(Vec<u8>, Vec<String>), Error> {
        let camera = self
            .camera
            .as_mut()
            .ok_or_else(|| Error::Other("The camera is not connected.".into()))?;
        camera.require_conversion_mode()?;

        // Upload the RAF unless the camera already holds it.
        if self.loaded.as_ref().map(|l| l.raf.as_path()) != Some(raf) {
            self.loaded = None;
            let bytes = std::fs::read(raf)
                .map_err(|e| Error::Other(format!("Could not read {}: {e}", raf.display())))?;
            camera.upload_raf(&bytes)?;
            // Reading the profile is also how the camera reports a RAF it cannot use.
            let as_shot = Profile::from_bytes(camera.read_profile()?)
                .ok_or_else(|| Error::Other("The camera sent a settings block that is too short.".into()))?;
            self.events.send(AppEvent::AsShot { raf: raf.to_path_buf(), profile: as_shot.clone() });
            self.loaded = Some(Loaded { raf: raf.to_path_buf(), as_shot });
            self.profile_changed = false;
        }
        let as_shot = &self.loaded.as_ref().unwrap().as_shot;

        let mut notes = Vec::new();
        match recipe {
            Some(job) => {
                let applied = apply::apply(&job.settings, as_shot);
                notes = applied.notes;
                // From here the camera's profile may differ from as shot, even if the write fails.
                self.profile_changed = true;
                camera.write_profile(&applied.profile.to_host_write()).map_err(|e| match e {
                    Error::Refused { code, .. } => Error::Other(format!(
                        "The camera refused the settings in \"{}\" (code {code:#06x}). \
                         It may use a setting this camera does not have.",
                        job.name
                    )),
                    e => e,
                })?;
                // Read back and report anything the camera did not take.
                if let Some(now) = camera.read_profile().ok().and_then(Profile::from_bytes) {
                    for (index, wanted, got) in applied.profile.differences(&now) {
                        let changed_by_recipe = as_shot.get(index) != Some(wanted);
                        if changed_by_recipe {
                            notes.push(format!(
                                "The camera did not accept {} (asked for {wanted}, it kept {got}).",
                                apply::cell_name(index)
                            ));
                        }
                    }
                }
            }
            None => {
                if self.profile_changed {
                    camera.write_profile(&as_shot.to_host_write())?;
                    self.profile_changed = false;
                }
            }
        }
        Ok((camera.develop(size)?, notes))
    }

    fn connected(&mut self) -> Result<&mut Camera<AnyTransport>, Error> {
        self.camera.as_mut().ok_or_else(|| Error::Other("The camera is not connected.".into()))
    }

    fn read_slots(&mut self) -> Result<Vec<SlotInfo>, Error> {
        let slots = self.connected()?.read_slots()?;
        Ok(slots.into_iter().map(|read| SlotInfo { settings: slot::settings_of(&read), read }).collect())
    }

    /// Stores a recipe in a custom slot: reads the slot, saves what it holds
    /// as a backup file, writes the name and settings, and reads them back.
    fn upload_to_slot(
        &mut self,
        number: u16,
        name: &str,
        job: &RecipeJob,
        backup_folder: &Path,
    ) -> Result<SlotReport, Error> {
        let camera = self.connected()?;
        let model = camera.info.model.clone();
        let before = camera.read_slot(number)?;

        // Back up first. If the backup cannot be saved, nothing is changed.
        let old_name = match before.name.trim() {
            "" => format!("C{number}"),
            name => name.to_string(),
        };
        let description = format!(
            "Backup of camera slot C{number} (\"{old_name}\"), saved before \"{}\" was uploaded to it.",
            job.name
        );
        let text = Recipe::build(&old_name, &model, &description, &slot::settings_of(&before).to_fields());
        let backup = library::write_recipe_file(backup_folder, &format!("C{number} backup - {old_name}"), &text)
            .map_err(|e| {
                Error::Other(format!("Could not save a backup of slot C{number}, so nothing was changed. {e}"))
            })?;

        let plan = slot::plan(&job.settings, &before);
        let wanted_name = slots::clean_name(name);
        let written = camera.write_slot(number, Some(&wanted_name), &plan.writes)?;

        let problems = slot::problems(&written.outcomes);
        let stored = written.outcomes.iter().filter(|o| slot::took(o)).count();
        let name_kept = written.name_after.trim() == wanted_name;
        let mut lines = vec![format!(
            "Uploaded \"{}\" to C{number}: {stored} of {} settings stored.",
            job.name,
            written.outcomes.len()
        )];
        if name_kept {
            lines.push(format!("The slot is now named \"{wanted_name}\"."));
        } else {
            lines.push(format!(
                "The name was not stored: the slot is called \"{}\" (answer code {:#06x}).",
                written.name_after.trim(),
                written.name_response.unwrap_or(0)
            ));
        }
        lines.extend(problems.iter().cloned());
        lines.extend(plan.notes.iter().cloned());
        lines.push(format!("What the slot held before was saved as \"{backup}\"."));
        Ok(SlotReport { complete: problems.is_empty() && name_kept, lines, backup: Some(backup) })
    }

    /// After a transport-level failure the session cannot be trusted: drop
    /// the connection and say so.
    fn drop_if_lost(&mut self, error: &Error) {
        let lost = matches!(error, Error::Timeout(_))
            || (matches!(error, Error::Other(_)) && self.camera.as_mut().map(|c| !c.is_alive()).unwrap_or(false));
        if lost && self.camera.is_some() {
            self.camera = None;
            self.loaded = None;
            self.events.log("Lost the connection to the camera.");
            self.events.send(AppEvent::Camera(CameraStatus::Failed(
                "The camera stopped answering. Switch it off and on, then connect again.".into(),
            )));
        }
    }

    fn fail(&mut self, key: PreviewKey, error: Error, export: bool) {
        // After a transport-level failure the session cannot be trusted.
        let lost = matches!(error, Error::Timeout(_))
            || (matches!(error, Error::Other(_)) && self.camera.as_mut().map(|c| !c.is_alive()).unwrap_or(false));
        self.events.send(AppEvent::DevelopFailed { key, message: error.to_string(), export });
        if lost && self.camera.is_some() {
            self.camera = None;
            self.loaded = None;
            self.events.log("Lost the connection to the camera.");
            self.events.send(AppEvent::Camera(CameraStatus::Failed(
                "The camera stopped answering. Switch it off and on, then connect again.".into(),
            )));
        }
    }
}

/// Saves a developed picture, turned as the user turned the photo and made
/// small, as a recipe's sample.
fn save_sample(jpeg: &[u8], orientation: Orientation, dest: &Path) -> Result<(), Error> {
    let recorded = xrp_camera::exif::parse_jpeg(jpeg).map(|e| e.orientation).unwrap_or(1);
    let image = image::load_from_memory(jpeg)
        .map_err(|e| Error::Other(format!("Could not read the developed image: {e}")))?;
    let image = orientation.apply(Orientation::from_exif(recorded).apply(image));
    let size = crate::samples::SAMPLE_SIZE;
    let small = image.thumbnail(size, size).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88)
        .encode_image(&small)
        .map_err(|e| Error::Other(format!("Could not save the sample picture: {e}")))?;
    if let Some(dir) = dest.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(dest, out).map_err(|e| Error::Other(format!("Could not save {}: {e}", dest.display())))
}

fn decode(jpeg: &[u8]) -> Result<ColorImage, Error> {
    let recorded = xrp_camera::exif::parse_jpeg(jpeg).map(|e| e.orientation).unwrap_or(1);
    let img = image::load_from_memory(jpeg)
        .map_err(|e| Error::Other(format!("Could not read the developed image: {e}")))?;
    let img = Orientation::from_exif(recorded).apply(img).to_rgb8();
    Ok(ColorImage::from_rgb([img.width() as usize, img.height() as usize], img.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::border::Border;

    fn finish(orientation: Orientation, border: Border) -> Finish {
        Finish { orientation, crop: None, border, options: Default::default() }
    }

    fn photo(orientation: u16) -> Vec<u8> {
        // A 4×2 picture: left half dark, right half bright.
        let mut img = image::RgbImage::new(4, 2);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = image::Rgb(if x < 2 { [10, 10, 10] } else { [240, 240, 240] });
        }
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100).encode_image(&img).unwrap();
        xrp_camera::exif::carry_over_upright(&xrp_camera::exif::test_jpeg(orientation, &[]), &jpeg)
            .into_iter()
            .collect::<Vec<u8>>()
    }

    #[test]
    fn a_sample_picture_is_saved_small_and_upright() {
        // A 1200×800 picture recorded as "turn 90° to view".
        let img = image::RgbImage::from_pixel(1200, 800, image::Rgb([120, 90, 60]));
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).encode_image(&img).unwrap();
        let jpeg = xrp_camera::exif::carry_over_upright(&xrp_camera::exif::test_jpeg(1, &[]), &jpeg);
        let dir = std::env::temp_dir().join(format!("xrb-sample-test-{}", std::process::id()));
        let dest = dir.join("Samples").join("Campfire.FP1.jpg");
        save_sample(&jpeg, Orientation::default().rotated_cw(), &dest).unwrap();
        let saved = image::open(&dest).unwrap();
        assert_eq!((saved.width(), saved.height()), (347, 520));
        assert!(save_sample(b"not a picture", Orientation::default(), &dest).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_newest_preview_request_per_copy_is_kept() {
        let job = |name: &str| {
            let mut settings = Settings::default();
            settings.highlight = Some(name.len() as i32);
            Some(RecipeJob { file_name: name.into(), name: name.into(), settings })
        };
        let preview = |slot: usize, recipe: Option<RecipeJob>| CameraCommand::Preview {
            slot,
            raf: PathBuf::from("/photos/a.RAF"),
            recipe,
        };
        let mut queue: std::collections::VecDeque<CameraCommand> = [
            preview(0, None),
            preview(1, job("one")),
            CameraCommand::Connect,
            preview(1, job("three")),
            preview(2, job("one")),
            preview(1, job("fifteen")),
        ]
        .into_iter()
        .collect();
        let skipped = drop_superseded_previews(&mut queue);

        let kept: Vec<String> = queue
            .iter()
            .map(|c| match c {
                CameraCommand::Preview { slot, recipe, .. } => {
                    format!("{slot}:{}", recipe.as_ref().map(|r| r.name.as_str()).unwrap_or("original"))
                }
                _ => "other".into(),
            })
            .collect();
        assert_eq!(kept, vec!["0:original", "other", "2:one", "1:fifteen"]);
        // "three" was replaced and nobody else wants it. "one" was replaced
        // for copy 1 but copy 2 still wants it, so it is not reported.
        let names: Vec<Option<String>> = skipped.into_iter().map(|k| k.recipe).collect();
        assert_eq!(names, vec![Some("three".to_string())]);
    }

    #[test]
    fn an_unturned_export_is_the_cameras_file_untouched() {
        let jpeg = photo(1);
        assert_eq!(finish(Orientation::default(), Border::default()).apply(jpeg.clone()).unwrap(), jpeg);
    }

    #[test]
    fn a_turned_export_has_turned_pixels_and_keeps_the_details() {
        let out = finish(Orientation::default().rotated_cw(), Border::default()).apply(photo(1)).unwrap();
        let image = image::load_from_memory(&out).unwrap().to_rgb8();
        assert_eq!(image.dimensions(), (2, 4), "a quarter turn swaps width and height");
        // The dark left half is now the top half.
        assert!(image.get_pixel(0, 0)[0] < 60 && image.get_pixel(1, 3)[0] > 190);
        let exif = xrp_camera::exif::parse_jpeg(&out).unwrap();
        assert_eq!((exif.model.as_str(), exif.orientation), ("X-T4", 1));

        // A 50% border on a 4×2 picture adds 1 pixel all round, in white.
        let framed =
            finish(Orientation::default(), Border { on: true, percent: 50.0, ..Default::default() }).apply(photo(1)).unwrap();
        let image = image::load_from_memory(&framed).unwrap().to_rgb8();
        assert_eq!((image.width(), image.height()), (6, 4));
        assert!(image.get_pixel(0, 0)[0] > 240 && image.get_pixel(1, 1)[0] < 60 && image.get_pixel(5, 3)[0] > 240);
        assert_eq!(xrp_camera::exif::parse_jpeg(&framed).unwrap().model, "X-T4");

        let mirrored = finish(Orientation::default().flipped_horizontally(), Border::default()).apply(photo(1)).unwrap();
        let image = image::load_from_memory(&mirrored).unwrap().to_rgb8();
        assert!(image.get_pixel(0, 0)[0] > 190 && image.get_pixel(3, 0)[0] < 60);
    }
}
