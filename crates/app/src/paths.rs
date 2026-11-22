//! Where the app keeps things on disk.

use std::path::{Path, PathBuf};

/// The recipes folder: a folder called `Recipes` next to the app itself, so
/// the app and its recipes stay together wherever they are kept.
///
/// - AppImage: next to the `.AppImage` file.
/// - Otherwise: next to the program file (`.exe` on Windows).
/// - If that place cannot be written to: `Documents/X Recipe Bench/Recipes`.
/// - `XRP_RECIPES_DIR`, when set, overrides all of these.
pub fn recipes_folder() -> PathBuf {
    if let Some(dir) = std::env::var_os("XRP_RECIPES_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let beside_app = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .and_then(|p| p.parent().map(|d| d.join("Recipes")));
    if let Some(folder) = beside_app {
        if is_writable(&folder) {
            return folder;
        }
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    // A library made under the app's old name keeps being used.
    let old = home.join("Documents").join("X Raw Probe").join("Recipes");
    let new = home.join("Documents").join(crate::APP_NAME).join("Recipes");
    if old.is_dir() && !new.exists() {
        old
    } else {
        new
    }
}

fn is_writable(folder: &Path) -> bool {
    if std::fs::create_dir_all(folder).is_err() {
        return false;
    }
    let probe = folder.join(".xrp-write-test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(probe);
    ok
}

/// Opens a folder in the system's file manager.
pub fn show_folder(folder: &Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(folder).spawn();
}
