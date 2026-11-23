//! X Recipe Bench: develop Fujifilm RAF files through the camera's own engine,
//! and build, organise and apply film recipes.
//!
//! An independent project, not affiliated with FUJIFILM Corporation.
//!
//! Where things live:
//! - `app.rs`      the window: menu bar, dock area, separate windows, event pump
//! - `dock.rs`     the panels, the three workspaces, and their arrangements
//! - `orientation.rs`  how a photo is turned (quarter turns and flips)
//! - `panels/`     one file per panel (Library, Preview, Camera, Log)
//! - `state.rs`    the data every panel reads and changes
//! - `events.rs`   messages sent from background threads to the window
//! - `camera_worker.rs`  background thread that owns the camera
//! - `image_worker.rs`   background thread that decodes pictures
//! - `settings.rs` what is remembered between runs
//! - `theme.rs`    dark and light modes and the five appearances
//! - `paths.rs`    where the recipes folder is

// On Windows, do not open a console window next to the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod border;
mod camera_worker;
mod chrome;
mod dock;
mod events;
mod export;
mod image_worker;
mod jpeg;
mod resize;
mod orientation;
mod panels;
mod paths;
mod photo;
mod samples;
mod pointer;
mod settings;
mod slots;
mod state;
mod theme;
mod windows;

pub const APP_NAME: &str = "X Recipe Bench";
pub const APP_ID: &str = "x-recipe-bench";

/// The app's icon, as a picture file built into the program.
pub const ICON_PNG: &[u8] = include_bytes!("../../../packaging/appimage/x-recipe-bench.png");

/// The icon in the form windows want it.
pub fn icon() -> std::sync::Arc<eframe::egui::IconData> {
    let data = match image::load_from_memory(ICON_PNG) {
        Ok(picture) => {
            let rgba = picture.to_rgba8();
            eframe::egui::IconData { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() }
        }
        Err(_) => eframe::egui::IconData::default(),
    };
    std::sync::Arc::new(data)
}

/// Who draws the title bar, decided once at startup by craft-window from the
/// system and the "Use the system title bar" setting. `XRB_TITLEBAR=app` or
/// `=system` overrides both.
pub fn title_bar(system: bool) -> craft_window::policy::Frame {
    use craft_window::policy::{decide_for, Backend, TitleBarPref};
    let pref = if system { TitleBarPref::System } else { TitleBarPref::Integrated };
    let frame = decide_for(craft_window::Prefix::new("XRB"), Backend::from_env(), pref).frame;
    match frame {
        // macOS keeps its own traffic lights; this app draws no inset bar.
        craft_window::policy::Frame::Inset(_) => craft_window::policy::Frame::Native,
        other => other,
    }
}

fn main() -> eframe::Result {
    let (store, notes) = settings::Store::open();
    let frame = title_bar(store.get().system_title_bar);
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_app_id(APP_ID)
        .with_icon(icon())
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([640.0, 400.0]);
    let options = eframe::NativeOptions {
        // Decorations on or off, as decided above (see chrome.rs for the app's own bar).
        viewport: craft_window::policy::apply(viewport, &frame),
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, store, notes, frame)))),
    )
}
