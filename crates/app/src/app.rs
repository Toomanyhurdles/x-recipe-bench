//! The window: menu bar with the workspace tabs, the dock area of the
//! current workspace, separate windows, and the event pump.

use crate::camera_worker::{self, CameraCommand};
use crate::chrome;
use crate::dock::{self, Panel};
use crate::events::{AppEvent, EmbeddedKind, EventSender};
use crate::image_worker;
use crate::settings::Settings;
use crate::theme::Appearance;
use crate::windows::{self, Place, Snapper};
use crate::paths;
use crate::state::{AppState, BatchLooks, CameraStatus, EditorState, RecipeState, Slot};
use xrp_recipes::Library;
use eframe::egui::{self, TextureOptions};
use std::collections::HashMap;
use std::sync::mpsc::Receiver;

pub struct App {
    state: AppState,
    /// Who draws the title bar (craft-window): the app, with these caption
    /// buttons, or the desktop.
    frame: craft_window::policy::Frame,
    /// The "Use the system title bar" setting, as it will be at the next start.
    system_title_bar: bool,
    /// The About window is open.
    about_open: bool,
    /// The app's icon, loaded once for drawing.
    icon_texture: Option<egui::TextureHandle>,
    icon: std::sync::Arc<egui::IconData>,
    /// The Photos panel is shown as a window of its own.
    photos_undocked: bool,
    /// Where that window is on the screen, as the system last reported.
    photos_place: Option<Place>,
    /// Where that window was told to open. Fixed while it is open (see
    /// `photos_window`).
    photos_opened_at: Option<Option<Place>>,
    /// Follows the windows on the screen: snapping and moving as a group.
    snapper: Snapper<egui::ViewportId>,
    /// The record of sample pictures has been read from disk.
    samples_loaded: bool,
    /// Frames drawn so far, for the steps taken as the app opens.
    frames: u32,
    /// A title-bar button of the main window was pressed this frame.
    window_button: Option<chrome::Pressed>,
    /// While the main window is maximised: where it was before.
    restore_place: Option<Place>,
    /// Whether the main window was minimised last frame.
    minimised: bool,
    /// Asks the system whether the mouse button is down (for snapping).
    buttons: crate::pointer::Buttons,
    /// Where the main window is, as the system last reported.
    main_place: Option<Place>,
    events: Receiver<AppEvent>,
    /// The settings file (craft-settings); writes only when something changed.
    store: crate::settings::Store,
    last_save_check: std::time::Instant,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        store: crate::settings::Store,
        load_notes: Vec<String>,
        frame: craft_window::policy::Frame,
    ) -> Self {
        let settings = store.get().clone();
        // craft-ui's font stack: egui's faces first, then the fonts installed
        // on the computer for any character they lack (arrows, stars, other scripts).
        craft_ui::fonts::UiFonts::new(&craft_ui::fonts::I18nScripts).install(&cc.egui_ctx);
        crate::theme::apply(&cc.egui_ctx, settings.appearance, settings.dark_mode);
        let (tx, rx) = std::sync::mpsc::channel();
        let sender = EventSender::new(tx, cc.egui_ctx.clone());

        let mut state = AppState {
            folder: None,
            files: Vec::new(),
            selected: None,
            info: None,
            orientations: settings
                .orientations
                .iter()
                .map(|(path, o)| (std::path::PathBuf::from(path), *o))
                .collect(),
            thumbnails: HashMap::new(),
            embedded: None,
            developed: Vec::new(),
            developing: Vec::new(),
            develop_errors: Vec::new(),
            // Always four copies to begin with, for comparing recipes.
            copies: 4,
            active_copy: 0,
            parked: Default::default(),
            as_shot: None,
            recipes: RecipeState::new(Library::open(&paths::recipes_folder())),
            editor: EditorState { auto_render: settings.auto_render, ..Default::default() },
            exporting: 0,
            camera: CameraStatus::Disconnected,
            log: Vec::new(),
            // Thumbnails always start small: 10% along the size slider.
            thumbnail_size: crate::panels::library::THUMBNAIL_SIZES.0
                + 0.10 * (crate::panels::library::THUMBNAIL_SIZES.1 - crate::panels::library::THUMBNAIL_SIZES.0),
            develop_on_select: settings.develop_on_select,
            connect_on_start: settings.connect_on_start,
            border: settings.border,
            export: settings.export.clone(),
            batch: Default::default(),
            zoom: 1.0,
            fit_scale: None,
            pan: egui::Vec2::ZERO,
            slots: Default::default(),
            stars: HashMap::new(),
            crops: HashMap::new(),
            facts: HashMap::new(),
            filter: Default::default(),
            notes_dirty: false,
            folder_writable: true,
            kept_notes: settings.photo_notes.clone(),
            crop_edit: None,
            samples: crate::samples::SamplesState {
                enabled: settings.samples_enabled,
                shared_photo: settings.sample_photo.clone(),
                own_photos: settings.sample_photos.clone(),
                ..Default::default()
            },
            snap_windows: settings.snap_windows,
            snap_distance: settings.snap_distance,
            appearance: settings.appearance,
            dark_mode: settings.dark_mode,
            settings_open: false,
            camera_tx: camera_worker::spawn(sender.clone()),
            image_tx: image_worker::spawn(sender.clone()),
            events: sender,
            dialogs: {
                let ctx = cc.egui_ctx.clone();
                craft_dialogs::Dialogs::new(std::sync::Arc::new(move || ctx.request_repaint()))
            },
        };
        state.log(format!("{} {}", crate::APP_NAME, env!("CARGO_PKG_VERSION")));
        for line in load_notes {
            state.log(line);
        }
        match &state.recipes.library {
            Ok(library) => {
                let line = format!(
                    "Recipes folder: {} ({} recipes).",
                    library.folder.display(),
                    library.recipes.len()
                );
                state.log(line);
            }
            Err(problem) => {
                let line = format!("Recipes folder problem: {problem}");
                state.log(line);
            }
        }

        // A folder or RAF given on the command line (or by "Open with") wins
        // over the folder remembered from last time.
        let argument = std::env::args_os().nth(1).map(std::path::PathBuf::from);
        match argument {
            Some(path) if path.is_dir() => state.open_folder(path),
            Some(path) if path.is_file() => {
                if let Some(parent) = path.parent() {
                    state.open_folder(parent.to_path_buf());
                }
                let full = state.files.iter().find(|f| f.file_name() == path.file_name()).cloned();
                if let Some(full) = full {
                    state.select(&full);
                }
            }
            _ => {
                if let Some(folder) = settings.last_folder.filter(|f| f.is_dir()) {
                    state.open_folder(folder);
                }
            }
        }
        if state.connect_on_start {
            state.send_camera(CameraCommand::Connect);
        }

        App {
            state,
            about_open: false,
            icon_texture: None,
            icon: crate::icon(),
            // The app always opens with the Photos panel docked.
            frame,
            system_title_bar: settings.system_title_bar,
            photos_undocked: false,
            photos_place: settings.photos_place,
            photos_opened_at: None,
            snapper: Snapper::default(),
            buttons: crate::pointer::Buttons::new(),
            samples_loaded: false,
            frames: 0,
            window_button: None,
            restore_place: None,
            minimised: false,
            main_place: None,
            events: rx,
            store,
            last_save_check: std::time::Instant::now(),
        }
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.events.try_recv() {
            let state = &mut self.state;
            match event {
                AppEvent::Camera(status) => {
                    let became_ready = status.ready() && !state.camera.ready();
                    state.camera = status;
                    if !state.camera.ready() {
                        state.developing.clear();
                    }
                    if became_ready && state.develop_on_select {
                        state.request_preview();
                    }
                }
                AppEvent::Developed { key, image, seconds, notes } => {
                    let name = key.raf.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let look = key.recipe.as_deref().unwrap_or("as shot");
                    state.log(format!(
                        "Developed {name} ({look}) in {seconds:.1}s ({}×{}).",
                        image.size[0], image.size[1]
                    ));
                    for note in &notes {
                        state.log(format!("Note: {note}"));
                    }
                    if let Some(recipe) = &key.recipe {
                        state.recipes.notes.insert(recipe.clone(), notes);
                    }
                    let texture = ctx.load_texture(format!("developed:{name}:{look}"), image, TextureOptions::LINEAR);
                    state.developing.retain(|k| k != &key);
                    state.store_developed(key, texture);
                }
                AppEvent::DevelopFailed { key, message, export } => {
                    if export {
                        state.exporting = state.exporting.saturating_sub(1);
                    } else {
                        state.developing.retain(|k| k != &key);
                    }
                    let verb = if export { "export" } else { "develop" };
                    state.log(format!("Could not {verb} {}: {message}", key.raf.display()));
                    state.develop_errors.retain(|(k, _)| k != &key);
                    state.develop_errors.push((key, message));
                    if state.develop_errors.len() > 16 {
                        state.develop_errors.remove(0);
                    }
                }
                AppEvent::PreviewSkipped(key) => {
                    // A newer request replaced it. If a copy still wants it
                    // (the user went back), it is asked for again.
                    state.developing.retain(|k| k != &key);
                    if (0..state.copies).any(|i| state.key_of(i).as_ref() == Some(&key)) {
                        state.request_preview();
                    }
                }
                AppEvent::AsShot { raf, profile } => state.as_shot = Some((raf, profile)),
                AppEvent::Exported { dest, seconds } => {
                    state.exporting = state.exporting.saturating_sub(1);
                    state.log(format!("Exported {} in {seconds:.1}s.", dest.display()));
                }
                AppEvent::Embedded { raf, kind, image } => {
                    let slot = match image {
                        Some(image) => Slot::Ready(ctx.load_texture(
                            format!("embedded:{}", raf.display()),
                            image,
                            TextureOptions::LINEAR,
                        )),
                        None => Slot::Missing,
                    };
                    match kind {
                        EmbeddedKind::Thumbnail => {
                            // Ignore thumbnails for a folder that is no longer open.
                            if state.thumbnails.contains_key(&raf) {
                                state.thumbnails.insert(raf, slot);
                            }
                        }
                        EmbeddedKind::Large => {
                            if state.selected.as_ref() == Some(&raf) {
                                state.embedded = Some((raf, slot));
                            }
                        }
                    }
                }
                AppEvent::FolderPicked(folder) => state.open_folder(folder),
                AppEvent::ExportDestPicked { raf, recipe, dest, finish } => {
                    state.exporting += 1;
                    state.log(format!("Exporting to {}…", dest.display()));
                    state.send_camera(CameraCommand::Export { raf, recipe, dest, finish });
                }
                AppEvent::SampleMade { recipe_file, result } => {
                    let Some((file, settings, photo)) = state.samples.in_flight.take() else { continue };
                    match result {
                        Ok(()) => {
                            let made = crate::samples::Made { settings, photo: photo.to_string_lossy().to_string() };
                            state.samples.index.samples.insert(file.clone(), made);
                            if let Some(folder) = state.recipes_folder() {
                                state.samples.index.save(&folder);
                            }
                            state.samples.reload(&file);
                        }
                        Err(problem) => {
                            state.log(format!("Could not make the sample picture for {recipe_file}: {problem}"));
                            state.samples.failed.push((file, settings, photo));
                        }
                    }
                }
                AppEvent::Facts(path, facts) => {
                    if path.parent() == state.folder.as_deref() {
                        state.facts.insert(path, facts);
                    }
                }
                AppEvent::BatchFolderPicked(dest) => state.start_batch(&dest),
                AppEvent::ExportsCancelled(count) => {
                    state.exporting = state.exporting.saturating_sub(count);
                    state.log(format!("Cancelled {count} exports that had not started."));
                }
                AppEvent::Slots(result) => {
                    state.slots.busy = false;
                    match result {
                        Ok(slots) => {
                            state.log(format!("Read {} custom slots from the camera.", slots.len()));
                            state.slots.list = Some(slots);
                            state.slots.error = None;
                        }
                        Err(problem) => {
                            state.log(format!("Could not read the camera's custom slots. {problem}"));
                            state.slots.list = None;
                            state.slots.error = Some(problem);
                        }
                    }
                }
                AppEvent::SlotWritten(result) => {
                    state.slots.busy = false;
                    let report = match result {
                        Ok(report) => report,
                        Err(problem) => crate::slots::SlotReport {
                            complete: false,
                            lines: vec![format!("The upload did not happen. {problem}")],
                            backup: None,
                        },
                    };
                    for line in &report.lines {
                        state.log(line.clone());
                    }
                    // File the backup under its own category.
                    if let (Some(backup), Ok(library)) = (&report.backup, &mut state.recipes.library) {
                        library.rescan();
                        if let Err(problem) = library.file_under(crate::slots::BACKUP_CATEGORY, backup) {
                            state.log.push(problem);
                        }
                    }
                    state.slots.report = Some(report);
                }
                AppEvent::RecipesPicked(files) => state.import_recipes(&files),
                AppEvent::PhotosPickedForRecipes(photos) => state.recipes_from_photos(&photos),
                AppEvent::RecipeExportFolderPicked(dest) => state.export_selected_recipes(&dest),
                AppEvent::Log(line) => state.log(line),
            }
        }
    }

    /// The main window's title bar: logo and name, menus, the workspace
    /// tabs, and the window buttons.
    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let captions = chrome::captions(&self.frame).cloned();
        // The bar's empty space moves the window, as a title bar does.
        if captions.is_some() {
            chrome::drag_area(ui);
        }
        let logo = self.logo(ui.ctx());
        let bar_centre = ui.max_rect().center().x;
        let maximised = self.restore_place.is_some() || ui.input(|i| i.viewport().maximized).unwrap_or(false);
        egui::MenuBar::new().ui(ui, |ui| {
            // Some desktops put the window buttons on the left.
            if let Some(captions) = captions.as_ref().filter(|c| !c.layout.left.is_empty()) {
                if let Some(pressed) = ui.scope(|ui| chrome::window_buttons(ui, &captions.layout.left, maximised)).inner {
                    self.window_button = Some(pressed);
                }
            }
            ui.add_space(4.0);
            ui.menu_button("File", |ui| {
                if ui.button("Open folder…").clicked() {
                    self.state.pick_folder();
                    ui.close();
                }
                let can_export = self.state.selected.is_some() && self.state.camera.ready();
                if ui.add_enabled(can_export, egui::Button::new("Export JPEG…")).clicked() {
                    self.state.export_selected();
                    ui.close();
                }
                if ui.button("Import recipe files…").clicked() {
                    self.state.pick_recipes_to_import();
                    ui.close();
                }
                if ui.button("Import a folder of recipes…").clicked() {
                    self.state.pick_recipe_folder_to_import();
                    ui.close();
                }
                if ui
                    .button("Restore starter recipes")
                    .on_hover_text("Puts back any starter recipes that were deleted. Existing files are not changed.")
                    .clicked()
                {
                    if let Ok(library) = &mut self.state.recipes.library {
                        let line = match library.install_starters() {
                            Ok(n) => format!("Restored {n} starter recipes."),
                            Err(e) => e,
                        };
                        self.state.log(line);
                    }
                    ui.close();
                }
                if ui.button("Batch export…").clicked() {
                    self.state.batch.open = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Settings…").clicked() {
                    self.state.settings_open = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("View", |ui| {
                let label = if self.photos_undocked { "Dock the Photos panel" } else { "Undock the Photos panel" };
                if ui.button(label).on_hover_text("Show the photo list in a window of its own, or put it back on the left.").clicked() {
                    self.photos_undocked = !self.photos_undocked;
                    ui.close();
                }
            });

            ui.menu_button("Help", |ui| {
                if ui.button(format!("About {}", crate::APP_NAME)).clicked() {
                    self.about_open = true;
                    ui.close();
                }
            });

            // The logo and name sit in the middle of the bar.
            let start = bar_centre - chrome::brand_width(ui) / 2.0;
            ui.add_space((start - ui.cursor().left()).max(12.0));
            chrome::brand(ui, &logo);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(captions) = captions.as_ref().filter(|c| !c.layout.right.is_empty()) {
                    if let Some(pressed) = ui.scope(|ui| chrome::window_buttons(ui, &captions.layout.right, maximised)).inner {
                        self.window_button = Some(pressed);
                    }
                }
                ui.add_space(6.0);
                if ui.button("Settings").on_hover_text("Appearance and behaviour.").clicked() {
                    self.state.settings_open = !self.state.settings_open;
                }
                ui.label(egui::RichText::new(self.state.camera.short_label()).weak());
            });
        });
    }

    /// The one fixed screen. From left to right: Photos (unless undocked);
    /// the strip with Info, Camera, Camera Slots and Log; the Preview; the
    /// Recipes list; the Editor. The lines between them can be dragged to
    /// change widths, but nothing can be closed or rearranged.
    fn fixed_screen(&mut self, ui: &mut egui::Ui) {
        let state = &mut self.state;
        if !self.photos_undocked {
            // Wide enough for three columns of thumbnails to begin with.
            let three_columns = 3.0 * state.thumbnail_size + 2.0 * crate::panels::GAP.x + 30.0;
            let mut undock = false;
            egui::Panel::left("photos_panel").resizable(true).default_size(three_columns).min_size(150.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    dock::heading(ui, Panel::Library);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        undock = ui
                            .small_button("Undock")
                            .on_hover_text("Show the photo list in a window of its own, which can go on another monitor.")
                            .clicked();
                    });
                });
                dock::show_panel(Panel::Library, ui, state);
            });
            if undock {
                self.photos_undocked = true;
            }
        }

        egui::Panel::left("strip").resizable(true).default_size(250.0).size_range(190.0..=420.0).show(ui, |ui| {
            // Three sections, top to bottom, each given a fixed share of
            // the strip's height and scrolling within it: Info; Camera with
            // Camera Slots; Log.
            let total = ui.available_height() - 2.0 * SECTION_GAP;
            let width = ui.available_width();
            let section = |ui: &mut egui::Ui, share: f32, add: &mut dyn FnMut(&mut egui::Ui)| {
                let height = (total * share).max(40.0);
                ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_min_height(height);
                    ui.set_max_height(height);
                    add(ui);
                });
            };
            section(ui, 0.30, &mut |ui| {
                dock::heading(ui, Panel::Info);
                dock::show_panel(Panel::Info, ui, state);
            });
            ui.separator();
            section(ui, 0.50, &mut |ui| {
                egui::ScrollArea::vertical().id_salt("settings_scroll").id_salt("camera_scroll").auto_shrink(false).show(ui, |ui| {
                    dock::heading(ui, Panel::Camera);
                    dock::show_panel(Panel::Camera, ui, state);
                    ui.add_space(6.0);
                    ui.separator();
                    dock::heading(ui, Panel::CameraSlots);
                    dock::show_panel(Panel::CameraSlots, ui, state);
                });
            });
            ui.separator();
            section(ui, 0.20, &mut |ui| {
                dock::heading(ui, Panel::Log);
                dock::show_panel(Panel::Log, ui, state);
            });
        });

        egui::Panel::right("editor_panel").resizable(true).default_size(340.0).size_range(280.0..=560.0).show(ui, |ui| {
            dock::heading(ui, Panel::Editor);
            dock::show_panel(Panel::Editor, ui, state);
        });
        // Only as wide as the longest recipe name needs.
        let recipes_width = crate::panels::recipes::width_for_names(ui, state);
        egui::Panel::right("recipes_panel").resizable(false).exact_size(recipes_width).show(ui, |ui| {
            dock::heading(ui, Panel::Recipes);
            dock::show_panel(Panel::Recipes, ui, state);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            dock::show_panel(Panel::Preview, ui, state);
        });
    }

    /// The Photos panel as a window of its own, while it is undocked; and
    /// keeping track of where the app's windows are, for snapping.
    fn photos_window(&mut self, ctx: &egui::Context) {
        let root = egui::ViewportId::ROOT;
        let id = photos_id();
        if self.photos_undocked {
            let mut builder = egui::ViewportBuilder::default()
                .with_title(format!("Photos – {}", crate::APP_NAME))
                .with_app_id(crate::APP_ID)
                .with_icon(self.icon.clone())
                // Like the main window: the app's title bar (with a close
                // button only) or the desktop's.
                .with_decorations(!self.frame_is_app_drawn())
                .with_inner_size([PHOTOS_WINDOW.0, PHOTOS_WINDOW.1]);
            // Reopen where it last was. The place is fixed when the window
            // opens and then left alone: if the description given to the
            // system changed every time the window moved, the system would
            // be told to move it back, and a dragged window would judder.
            let place = *self.photos_opened_at.get_or_insert(self.photos_place);
            if let Some(place) = place {
                builder = builder.with_position([place.x, place.y]).with_inner_size([place.width, place.height]);
            }
            let logo = self.logo(ctx);
            let app_drawn = self.frame_is_app_drawn();
            let state = &mut self.state;
            let mut dock = false;
            ctx.show_viewport_immediate(id, builder, |ui, _class| {
                egui::Panel::top("photos_window_bar").show(ui, |ui| {
                    if app_drawn {
                        chrome::drag_area(ui);
                    }
                    ui.horizontal(|ui| {
                        let size = egui::TextStyle::Body.resolve(ui.style()).size;
                        ui.add(egui::Image::new(&logo).fit_to_exact_size(egui::Vec2::splat(size * 1.3)));
                        ui.strong("Photos");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if app_drawn
                                && ui.scope(|ui| chrome::window_buttons(ui, &[craft_window::Button::Close], false)).inner
                                    == Some(chrome::Pressed::Close)
                            {
                                dock = true;
                            }
                            ui.add_space(6.0);
                            dock |= ui.button("Dock").on_hover_text("Put the photo list back on the left of the main window.").clicked();
                        });
                    });
                });
                egui::CentralPanel::default().show(ui, |ui| {
                    dock::show_panel(Panel::Library, ui, state);
                });
                // Closing the window docks the panel again.
                dock |= ui.input(|i| i.viewport().close_requested());
                if app_drawn {
                    chrome::resize_edges(ui.ctx());
                    chrome::outline(ui.ctx());
                }
            });
            if dock {
                self.photos_undocked = false;
            }
        }
        if !self.photos_undocked {
            self.photos_opened_at = None;
        }

        // Where the windows are: remembered, and used for snapping.
        self.main_place = windows::place_of(ctx, root).or(self.main_place);
        let mut frames = Vec::new();
        if let Some(frame) = windows::frame_of(ctx, root) {
            frames.push((root, frame));
        }
        if self.photos_undocked {
            self.photos_place = windows::place_of(ctx, id).or(self.photos_place);
            if let Some(frame) = windows::frame_of(ctx, id) {
                frames.push((id, frame));
            }
        }
        // With one window there is nothing to snap to; do not ask about the mouse.
        let button = if frames.len() > 1 && self.state.snap_windows { self.buttons.left_down() } else { Some(false) };
        let moves = self.snapper.update(
            std::time::Instant::now(),
            &frames,
            button,
            self.state.snap_windows,
            self.state.snap_distance.clamp(4.0, 100.0),
        );
        for (window, x, y) in moves {
            windows::move_to(ctx, window, x, y);
        }
        if self.snapper.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    /// The logo as a picture the interface can draw.
    fn logo(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        self.icon_texture
            .get_or_insert_with(|| {
                let picture = image::load_from_memory(crate::ICON_PNG).map(|p| p.to_rgba8()).unwrap_or_default();
                let size = [picture.width() as usize, picture.height() as usize];
                ctx.load_texture("logo", egui::ColorImage::from_rgba_unmultiplied(size, picture.as_raw()), TextureOptions::LINEAR)
            })
            .clone()
    }

    /// The app draws the title bar (rather than the desktop).
    fn frame_is_app_drawn(&self) -> bool {
        chrome::captions(&self.frame).is_some()
    }

    /// Carries out a press of one of the main window's title-bar buttons,
    /// and keeps the panel windows minimised or shown along with the main window.
    fn window_buttons(&mut self, ctx: &egui::Context) {
        let root = egui::ViewportId::ROOT;
        match self.window_button.take() {
            Some(chrome::Pressed::Close) => ctx.send_viewport_cmd_to(root, egui::ViewportCommand::Close),
            Some(chrome::Pressed::Minimise) => ctx.send_viewport_cmd_to(root, egui::ViewportCommand::Minimized(true)),
            Some(chrome::Pressed::Maximise) => self.toggle_maximise(ctx),
            None => {}
        }
        // However the main window was minimised or brought back (its own
        // button, the taskbar, a keyboard shortcut), the panels follow.
        let minimised = ctx.input_for(root, |i| i.viewport().minimized).unwrap_or(false);
        if minimised != self.minimised {
            self.minimised = minimised;
            if self.photos_undocked {
                ctx.send_viewport_cmd_to(photos_id(), egui::ViewportCommand::Minimized(minimised));
            }
            if !minimised {
                ctx.send_viewport_cmd_to(root, egui::ViewportCommand::Focus);
            }
        }
    }

    /// At startup: puts the main window, at a moderate size, on the monitor
    /// the mouse pointer is on (where the app was started from), and then
    /// asks the system to maximise it there. Leaving the maximising to the
    /// system means the system also knows how to undo it: the app's own
    /// restore button, the desktop's shortcuts and dragging the title bar
    /// all bring back the moderate size.
    fn open_large(&mut self, ctx: &egui::Context) {
        const PLACE_AT: u32 = 6;
        const MAXIMISE_AT: u32 = 18;
        if self.frames > MAXIMISE_AT {
            return;
        }
        self.frames += 1;
        ctx.request_repaint();
        let root = egui::ViewportId::ROOT;
        if self.frames == PLACE_AT {
            if let Some(now) = windows::frame_of(ctx, root) {
                let scale = ctx.pixels_per_point();
                // The monitor with the mouse pointer; failing that, the window's.
                let centre = ((now.x + now.w / 2.0) * scale, (now.y + now.h / 2.0) * scale);
                let point = self.buttons.pointer_position().unwrap_or(centre);
                if let Some((x, y, w, h)) = self.buttons.work_area(point.0, point.1) {
                    let area = windows::Frame { x: x / scale, y: y / scale, w: w / scale, h: h / scale };
                    self.snapper.look_away(std::time::Instant::now());
                    // 70% of the monitor's usable area, in its middle.
                    windows::move_to(ctx, root, area.x + area.w * 0.15, area.y + area.h * 0.15);
                    ctx.send_viewport_cmd_to(root, egui::ViewportCommand::InnerSize(egui::vec2(area.w * 0.7, area.h * 0.7)));
                }
            }
        }
        if self.frames == MAXIMISE_AT {
            self.snapper.look_away(std::time::Instant::now());
            ctx.send_viewport_cmd_to(root, egui::ViewportCommand::Maximized(true));
        }
    }

    /// Maximises the main window into the largest part of its monitor that
    /// the panel windows do not cover, or puts it back as it was.
    fn toggle_maximise(&mut self, ctx: &egui::Context) {
        let root = egui::ViewportId::ROOT;
        let place = |target: Place| {
            windows::move_to(ctx, root, target.x, target.y);
            ctx.send_viewport_cmd_to(root, egui::ViewportCommand::InnerSize(egui::vec2(target.width, target.height)));
        };
        self.snapper.look_away(std::time::Instant::now());
        // Maximised by the system (at startup, or by the desktop's own
        // shortcut): only the system can undo that. While it holds, the
        // system ignores the app's own requests to resize the window.
        if ctx.input_for(root, |i| i.viewport().maximized).unwrap_or(false) {
            self.restore_place = None;
            ctx.send_viewport_cmd_to(root, egui::ViewportCommand::Maximized(false));
            return;
        }
        if let Some(before) = self.restore_place.take() {
            place(before);
            return;
        }
        let Some(now) = windows::frame_of(ctx, root) else {
            // The system does not say where windows are (Wayland): let it maximise.
            let maximised = ctx.input_for(root, |i| i.viewport().maximized).unwrap_or(false);
            ctx.send_viewport_cmd_to(root, egui::ViewportCommand::Maximized(!maximised));
            return;
        };
        // The monitor's usable area, in the interface's units.
        let scale = ctx.pixels_per_point();
        let centre = ((now.x + now.w / 2.0) * scale, (now.y + now.h / 2.0) * scale);
        let area = match self.buttons.work_area(centre.0, centre.1) {
            Some((x, y, w, h)) => windows::Frame { x: x / scale, y: y / scale, w: w / scale, h: h / scale },
            None => {
                let size = ctx.input_for(root, |i| i.viewport().monitor_size).unwrap_or(egui::vec2(now.w, now.h));
                windows::Frame { x: 0.0, y: 0.0, w: size.x, h: size.y }
            }
        };
        let panels: Vec<windows::Frame> =
            self.photos_undocked.then(|| windows::frame_of(ctx, photos_id())).flatten().into_iter().collect();
        let mut free = windows::largest_free(area, &panels);
        // If the panels leave no useful room, use the whole area.
        if free.w < 640.0 || free.h < 400.0 {
            free = area;
        }
        self.restore_place = self.main_place.or(Some(Place { x: now.x, y: now.y, width: now.w, height: now.h }));
        place(Place { x: free.x, y: free.y, width: free.w, height: free.h });
    }

    /// The recipes that should have a sample picture, each with the settings
    /// fingerprint and photo it should be made from. Backups of camera
    /// slots and recipes that set nothing are left out.
    fn sample_wants(&self) -> Vec<(String, u64, std::path::PathBuf)> {
        let Ok(library) = &self.state.recipes.library else { return Vec::new() };
        library
            .recipes
            .iter()
            .filter(|e| e.recipe.is_ok() && !e.settings.is_empty())
            .filter(|e| !library.is_in(crate::slots::BACKUP_CATEGORY, &e.file_name))
            .filter_map(|e| {
                let photo = self.state.samples.photo_for(&e.file_name)?;
                Some((e.file_name.clone(), e.settings.fingerprint(), photo.clone()))
            })
            .collect()
    }

    /// How many sample pictures are made, out of how many wanted.
    fn sample_progress(&self) -> (usize, usize) {
        let Some(folder) = self.state.recipes_folder() else { return (0, 0) };
        let wants = self.sample_wants();
        let done =
            wants.iter().filter(|(f, s, p)| self.state.samples.index.is_current(&folder, f, *s, p)).count();
        (done, wants.len())
    }

    /// Asks the camera for the next missing sample picture, one at a time
    /// and only while nothing else is waiting for the camera, so the user's
    /// own previews and exports always go first. Also loads finished
    /// pictures for showing.
    fn make_samples(&mut self, ctx: &egui::Context) {
        if !self.state.samples.enabled {
            return;
        }
        let Some(folder) = self.state.recipes_folder() else { return };
        if !self.samples_loaded {
            self.samples_loaded = true;
            self.state.samples.index = crate::samples::Index::load(&folder);
        }
        // Load a few finished pictures each frame.
        let mut loaded = 0;
        let files: Vec<String> = self.state.samples.index.samples.keys().cloned().collect();
        for file in files {
            if loaded == 3 {
                ctx.request_repaint();
                break;
            }
            if self.state.samples.textures.contains_key(&file) || self.state.samples.unreadable.contains(&file) {
                continue;
            }
            loaded += 1;
            let picture = std::fs::read(crate::samples::picture(&folder, &file)).ok().and_then(|b| crate::jpeg::decode(&b).ok());
            match picture {
                Some(picture) => {
                    let rgb = picture.to_rgb8();
                    let image = egui::ColorImage::from_rgb([rgb.width() as usize, rgb.height() as usize], rgb.as_raw());
                    let texture = ctx.load_texture(format!("sample:{file}"), image, TextureOptions::LINEAR);
                    self.state.samples.textures.insert(file, texture);
                }
                None => self.state.samples.unreadable.push(file),
            }
        }

        let state = &self.state;
        let idle = state.camera.ready()
            && state.samples.in_flight.is_none()
            && state.developing.is_empty()
            && state.exporting == 0
            && !state.slots.busy
            && state.editor.pending_since.is_none();
        if !idle {
            return;
        }
        let next = self.sample_wants().into_iter().find(|(file, settings, photo)| {
            photo.is_file()
                && !state.samples.index.is_current(&folder, file, *settings, photo)
                && !state.samples.failed.iter().any(|(f, s, p)| f == file && s == settings && p == photo)
        });
        let Some((file, settings, photo)) = next else { return };
        let Ok(library) = &state.recipes.library else { return };
        let Some(entry) = library.find(&file) else { return };
        let job = crate::camera_worker::RecipeJob {
            file_name: entry.file_name.clone(),
            name: entry.display_name(),
            settings: entry.settings.clone(),
        };
        state.send_camera(CameraCommand::Sample {
            raf: photo.clone(),
            job,
            dest: crate::samples::picture(&folder, &file),
            orientation: state.orientation_of(&photo),
        });
        self.state.samples.in_flight = Some((file, settings, photo));
    }

    /// The warning shown before sample pictures are switched on.
    fn samples_warning(&mut self, ctx: &egui::Context) {
        if !self.state.samples.asking {
            return;
        }
        let count = match &self.state.recipes.library {
            Ok(library) => library.recipes.len(),
            Err(_) => 0,
        };
        let state = &mut self.state;
        let mut open = true;
        egui::Window::new("Switch on recipe sample pictures?")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.label(
                    "Each recipe gets a small picture showing what it does to a photo of yours. Only the camera \
                     can apply a recipe, so the camera has to develop the photo once for every recipe.",
                );
                ui.add_space(6.0);
                ui.label(egui::RichText::new("The first time takes a while").strong());
                ui.label(format!(
                    "A few seconds per recipe: for your {count} recipes, expect roughly {} with the camera connected. \
                     It runs in the background and steps aside whenever you develop or export something yourself.",
                    rough_time(count)
                ));
                ui.add_space(6.0);
                ui.label(egui::RichText::new("After that there is no delay").strong());
                ui.label(
                    "The pictures are kept in the Samples folder inside your Recipes folder and shown instantly, \
                     with or without the camera. One is made again only when its recipe changes or you choose a \
                     different photo for it.",
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Switch on").clicked() {
                        state.samples.enabled = true;
                        state.samples.asking = false;
                        // Start with the photo that is selected, if none was chosen before.
                        if state.samples.shared_photo.is_none() {
                            state.samples.shared_photo = state.selected.clone();
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        state.samples.asking = false;
                    }
                });
            });
        if !open {
            state.samples.asking = false;
        }
    }

    /// Batch export: several photos, several looks, one folder.
    fn batch_window(&mut self, ctx: &egui::Context) {
        let state = &mut self.state;
        if state.batch.running && state.exporting == 0 {
            state.batch.running = false;
            state.log("Batch export finished.");
        }
        if !state.batch.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Batch export")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(ctx.content_rect().center() - egui::vec2(220.0, 200.0))
            .show(ctx, |ui| {
                ui.set_width(440.0);
                let running = state.batch.running;
                ui.add_enabled_ui(!running, |ui| {
                    ui.label(egui::RichText::new("Photos").strong());
                    let one = match &state.selected {
                        Some(path) => format!(
                            "The selected photo ({})",
                            path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                        ),
                        None => "The selected photo (none selected)".to_string(),
                    };
                    ui.radio_value(&mut state.batch.all_photos, false, one);
                    let all = format!("All {} photos shown in Browse", state.visible_files().len());
                    ui.radio_value(&mut state.batch.all_photos, true, all);
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Looks").strong());
                    let current = format!("The current look ({})", state.look_name());
                    ui.radio_value(&mut state.batch.looks, BatchLooks::Highlighted, current);
                    ui.radio_value(
                        &mut state.batch.looks,
                        BatchLooks::AllCopies,
                        format!("Every look shown in the Preview ({} copies)", state.copies),
                    );
                });
                ui.add_space(6.0);
                let (photos, looks) = (state.batch_photos().len(), state.batch_looks().len());
                let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
                ui.label(format!(
                    "{} × {} = {}",
                    plural(photos, "photo"),
                    plural(looks, "look"),
                    plural(photos * looks, "JPEG")
                ));
                crate::panels::hint(
                    ui,
                    "Each JPEG is developed by the camera at full size. Rotation, the border and the export \
                     options in Settings apply. Existing files are never replaced.",
                );
                ui.add_space(6.0);
                if running {
                    let done = state.batch.total.saturating_sub(state.exporting);
                    ui.add(
                        egui::ProgressBar::new(done as f32 / state.batch.total.max(1) as f32)
                            .text(format!("{done} of {}", state.batch.total)),
                    );
                    if ui.button("Cancel the rest").clicked() {
                        state.send_camera(CameraCommand::CancelExports);
                    }
                } else {
                    let ready = state.camera.ready() && photos * looks > 0;
                    let button = ui.add_enabled(ready, egui::Button::new("Choose folder and export…"));
                    if button.on_disabled_hover_text("Connect the camera, and choose at least one photo and look.").clicked() {
                        state.pick_batch_folder();
                    }
                }
            });
        if !open {
            state.batch.open = false;
        }
    }

    /// The Settings window: appearance, and the app's behaviour options.
    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.state.settings_open {
            return;
        }
        let sample_progress = if self.state.samples.enabled { self.sample_progress() } else { (0, 0) };
        let state = &mut self.state;
        let (mut appearance, mut dark) = (state.appearance, state.dark_mode);
        let mut open = true;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(egui::pos2((ctx.content_rect().center().x - 240.0).max(0.0), 36.0))
            .show(ctx, |ui| {
                ui.set_width(470.0);
                // Taller than a small screen: the contents scroll.
                let room = ctx.content_rect().height() - 90.0;
                egui::ScrollArea::vertical().max_height(room).min_scrolled_height(room).show(ui, |ui| {
                ui.label(egui::RichText::new("Mode").strong());
                ui.horizontal(|ui| {
                    ui.radio_value(&mut dark, true, "Dark");
                    ui.radio_value(&mut dark, false, "Light");
                });
                ui.add_space(8.0);

                ui.label(egui::RichText::new("Appearance").strong());
                for choice in Appearance::ALL {
                    ui.radio_value(&mut appearance, choice, choice.title());
                    ui.indent(("appearance", choice.title()), |ui| {
                        ui.label(egui::RichText::new(choice.description()).weak());
                    });
                }
                ui.add_space(8.0);
                ui.separator();

                ui.label(egui::RichText::new("Behaviour").strong());
                ui.checkbox(&mut state.connect_on_start, "Connect to the camera when the app starts");
                ui.checkbox(&mut state.develop_on_select, "Develop a photo as soon as it is selected");
                ui.checkbox(&mut state.editor.auto_render, "Auto-render: develop again shortly after each change in the Editor");
                ui.checkbox(&mut self.system_title_bar, "Use the system title bar")
                    .on_hover_text("Let the desktop draw the window's title bar and buttons instead of the app. Takes effect the next time the app starts.");
                ui.add_space(8.0);
                ui.separator();

                ui.label(egui::RichText::new("Recipe sample pictures").strong());
                let mut on = state.samples.enabled;
                if ui
                    .checkbox(&mut on, "Show a sample picture for each recipe")
                    .on_hover_text("Shown when the mouse is over a recipe, and on the tiles of the Recipe Manager's grid view.")
                    .changed()
                {
                    if on {
                        // Switched on only after the warning is accepted.
                        state.samples.asking = true;
                    } else {
                        state.samples.enabled = false;
                    }
                }
                if state.samples.enabled {
                    let name = |p: &std::path::PathBuf| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    ui.horizontal(|ui| {
                        ui.label("Photo for all recipes:");
                        match &state.samples.shared_photo {
                            Some(photo) => ui.label(egui::RichText::new(name(photo)).strong()),
                            None => ui.colored_label(ui.visuals().warn_fg_color, "none chosen"),
                        };
                        let selected = state.selected.clone();
                        if ui
                            .add_enabled(selected.is_some(), egui::Button::new("Use the selected photo"))
                            .on_disabled_hover_text("Select a photo in the Browse tab first.")
                            .clicked()
                        {
                            state.samples.shared_photo = selected;
                        }
                    });
                    let own = state.samples.own_photos.len();
                    ui.horizontal(|ui| {
                        ui.label(format!("Recipes with a photo of their own: {own}"));
                        if own > 0 && ui.button("Use the shared photo for all").clicked() {
                            state.samples.own_photos.clear();
                        }
                    });
                    crate::panels::hint(ui, "To give one recipe its own photo: select the photo, then use the recipe's \"…\" menu in the Recipe Manager.");
                    let (done, total) = sample_progress;
                    ui.horizontal(|ui| {
                        if total > 0 {
                            ui.add(egui::ProgressBar::new(done as f32 / total as f32).desired_width(200.0).text(format!("{done} of {total} made")));
                        }
                        if done < total && !state.camera.ready() {
                            ui.label(egui::RichText::new("Connect the camera to make the rest.").weak());
                        }
                        if ui.button("Make all again").on_hover_text("Throw away the pictures made so far and make every one afresh.").clicked() {
                            state.samples.index.samples.clear();
                            state.samples.failed.clear();
                            state.samples.textures.clear();
                            state.samples.unreadable.clear();
                            if let Some(folder) = state.recipes_folder() {
                                state.samples.index.save(&folder);
                            }
                        }
                    });
                }
                ui.add_space(8.0);
                ui.separator();

                ui.label(egui::RichText::new("Export").strong());
                ui.horizontal(|ui| {
                    ui.label("File names:");
                    ui.add(egui::TextEdit::singleline(&mut state.export.naming).desired_width(200.0))
                        .on_hover_text("{photo} is the RAF's name, {recipe} the recipe's name.");
                    ui.label(
                        egui::RichText::new(format!("e.g. {}", state.export.file_name("DSCF2002", Some("Campfire")))).weak(),
                    );
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut state.export.resize, "Resize so the longer side is");
                    ui.add_enabled(
                        state.export.resize,
                        egui::DragValue::new(&mut state.export.long_edge).range(64..=20000).speed(10).suffix(" px"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("JPEG quality when the app re-saves:");
                    ui.add(egui::DragValue::new(&mut state.export.quality).range(50..=100));
                })
                .response
                .on_hover_text("Used only for photos that are turned, bordered or resized. Others are saved exactly as the camera made them.");
                ui.add_space(8.0);
                ui.separator();

                ui.label(egui::RichText::new("Windows").strong());
                ui.checkbox(&mut state.snap_windows, "Separate windows snap together and move as a group")
                    .on_hover_text("Drop a window near another window's edge and it jumps flush against it. Windows that touch move together; a quick pull takes one out of its group.");
                ui.add_enabled_ui(state.snap_windows, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Snap when closer than");
                        ui.add(egui::DragValue::new(&mut state.snap_distance).range(4.0..=100.0).speed(0.5).suffix(" px"));
                    });
                });
                ui.add_space(8.0);
                ui.separator();

                ui.label(egui::RichText::new("Recipes folder").strong());
                match &state.recipes.library {
                    Ok(library) => {
                        ui.label(library.folder.display().to_string());
                        if ui.button("Open folder").clicked() {
                            paths::show_folder(&library.folder);
                        }
                    }
                    Err(problem) => {
                        ui.colored_label(ui.visuals().warn_fg_color, problem);
                    }
                }
                });
            });
        if !open {
            state.settings_open = false;
        }
        if (appearance, dark) != (state.appearance, state.dark_mode) {
            state.appearance = appearance;
            state.dark_mode = dark;
            crate::theme::apply(ctx, appearance, dark);
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let texture = self.icon_texture.get_or_insert_with(|| {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [self.icon.width as usize, self.icon.height as usize],
                &self.icon.rgba,
            );
            ctx.load_texture("app-icon", image, TextureOptions::LINEAR)
        });
        let mut open = true;
        egui::Window::new(format!("About {}", crate::APP_NAME))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new((texture.id(), egui::vec2(72.0, 72.0))));
                    ui.vertical(|ui| {
                        ui.heading(crate::APP_NAME);
                        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                        ui.label("Develops Fujifilm RAF files through the camera's own processor, and builds, organises and applies film recipes.");
                    });
                });
                ui.separator();
                ui.label("An independent project. Not affiliated with or endorsed by FUJIFILM Corporation. FUJIFILM and X RAW STUDIO are trademarks of their owner.");
                ui.label("Software that talks to a camera over USB is used at your own risk.");
                ui.separator();
                ui.label(egui::RichText::new("Credits").strong());
                ui.label("Protocol notes: fujifilm-ptp-reference by zgrimz (CC BY 4.0).");
                ui.label("Recipe file vocabulary: the libfuji project (MIT).");
                ui.label("Setting names: the libgphoto2 project.");
                ui.label("Inspired by grawji, FilmKit and X RAW STUDIO.");
            });
        if !open {
            self.about_open = false;
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        // Arrow keys move through the library unless something is being typed.
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (previous, next) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowDown),
            )
        });
        // 1 to 5 give the selected photo that many stars; 0 takes them away.
        let digit = ctx.input(|i| {
            [egui::Key::Num0, egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5]
                .iter()
                .position(|key| i.key_pressed(*key) && i.modifiers.is_none())
        });
        if let (Some(stars), Some(raf)) = (digit, self.state.selected.clone()) {
            self.state.set_stars(&raf, stars as u8);
        }
        if previous {
            self.state.select_relative(-1);
        }
        if next {
            self.state.select_relative(1);
        }
    }

    /// Picks up recipe files added, removed or edited outside the app.
    fn watch_recipes_folder(&mut self) {
        let Ok(library) = &mut self.state.recipes.library else { return };
        if library.changed_on_disk() {
            library.rescan();
            let line = format!("Recipes folder changed: {} recipes.", library.recipes.len());
            self.state.refresh_editor_from_library();
            self.state.log(line);
        }
    }

    /// Recipe files dropped onto the window are imported; a dropped RAF or
    /// folder is opened.
    fn dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<std::path::PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if dropped.is_empty() {
            return;
        }
        let has_extension = |p: &std::path::Path, ext: &str| {
            p.extension().map(|x| x.to_string_lossy().eq_ignore_ascii_case(ext)).unwrap_or(false)
        };
        let recipes: Vec<_> = dropped.iter().filter(|p| has_extension(p, "fp1")).cloned().collect();
        if !recipes.is_empty() {
            self.state.import_recipes(&recipes);
        }
        if let Some(folder) = dropped.iter().find(|p| p.is_dir()) {
            // A folder with photos in it is opened; any other folder is
            // searched for recipes to import.
            let has_photos = std::fs::read_dir(folder)
                .map(|entries| entries.flatten().any(|e| has_extension(&e.path(), "raf")))
                .unwrap_or(false);
            if has_photos {
                self.state.open_folder(folder.clone());
            } else {
                self.state.import_recipes(std::slice::from_ref(folder));
            }
        } else if let Some(raf) = dropped.iter().find(|p| has_extension(p, "raf")) {
            if let Some(parent) = raf.parent() {
                if self.state.folder.as_deref() != Some(parent) {
                    self.state.open_folder(parent.to_path_buf());
                }
            }
            self.state.select(raf);
        }
    }

    /// Renders a little after the last slider movement, so dragging a slider
    /// does not ask the camera for every value on the way.
    fn auto_render(&mut self, ctx: &egui::Context) {
        let Some(since) = self.state.editor.pending_since else { return };
        let wait = std::time::Duration::from_millis(350);
        if since.elapsed() >= wait {
            self.state.editor.pending_since = None;
            if self.state.editor.auto_render {
                self.state.request_preview();
            }
        } else {
            ctx.request_repaint_after(wait - since.elapsed());
        }
    }

    fn save_if_changed(&mut self) {
        self.last_save_check = std::time::Instant::now();
        let settings = self.settings();
        self.store.save(settings);
    }

    fn settings(&self) -> Settings {
        Settings {
            last_folder: self.state.folder.clone(),
            photos_place: self.photos_place,
            photo_notes: self.state.kept_notes.clone(),
            samples_enabled: self.state.samples.enabled,
            sample_photo: self.state.samples.shared_photo.clone(),
            sample_photos: self.state.samples.own_photos.clone(),
            snap_windows: self.state.snap_windows,
            snap_distance: self.state.snap_distance,
            border: self.state.border,
            export: self.state.export.clone(),
            // Turns of photos in the open folder live in that folder's own
            // file; only the others are kept here.
            orientations: self
                .state
                .orientations
                .iter()
                .filter(|(path, _)| !(self.state.folder_writable && path.parent() == self.state.folder.as_deref()))
                .map(|(path, o)| (path.to_string_lossy().to_string(), *o))
                .collect(),
            preview_copies: self.state.copies,
            thumbnail_size: self.state.thumbnail_size,
            develop_on_select: self.state.develop_on_select,
            connect_on_start: self.state.connect_on_start,
            auto_render: self.state.editor.auto_render,
            appearance: self.state.appearance,
            dark_mode: self.state.dark_mode,
            system_title_bar: self.system_title_bar,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.state.collect_dialog_answers();
        self.handle_events(&ctx);
        self.keyboard(&ctx);
        self.dropped_files(&ctx);
        self.auto_render(&ctx);

        egui::Panel::top("menu_bar").show(ui, |ui| self.menu_bar(ui));
        self.fixed_screen(ui);
        self.photos_window(&ctx);
        self.open_large(&ctx);
        self.window_buttons(&ctx);
        if self.frame_is_app_drawn() {
            chrome::resize_edges(&ctx);
            chrome::outline(&ctx);
        }
        self.settings_window(&ctx);
        self.batch_window(&ctx);
        self.make_samples(&ctx);
        if self.state.notes_dirty {
            self.state.save_notes();
        }
        self.samples_warning(&ctx);
        self.about_window(&ctx);

        // Save settings shortly after they change, so they survive a crash.
        let closing = ctx.input(|i| i.viewport().close_requested());
        if closing || self.last_save_check.elapsed().as_secs() >= 2 {
            self.watch_recipes_folder();
            self.save_if_changed();
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
        // File dialogs asked for during this frame open now, over the main window.
        self.state.dialogs.show_pending(Some(&*frame));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_if_changed();
    }
}

/// The id of the Photos panel's own window.
fn photos_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("photos_window")
}

/// "about 2 minutes" for this many recipes, at a few seconds each.
fn rough_time(recipes: usize) -> String {
    let seconds = recipes * 4;
    if seconds < 90 {
        "under two minutes".to_string()
    } else {
        format!("{} to {} minutes", (seconds / 90).max(1), seconds.div_ceil(40))
    }
}

/// The size the Photos panel's own window opens at the first time.
const PHOTOS_WINDOW: (f32, f32) = (460.0, 900.0);

/// Height taken by the line between two sections of the strip.
const SECTION_GAP: f32 = 8.0;
