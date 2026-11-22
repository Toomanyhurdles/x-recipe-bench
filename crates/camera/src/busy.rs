//! Finding out which other program has the camera open.
//!
//! On Linux a program using a USB device holds `/dev/bus/usb/BBB/DDD` open,
//! which shows up under `/proc/<pid>/fd`. Other systems: not implemented yet.

/// Name of a process holding the device, e.g. "gvfsd-gphoto2 (pid 2314)".
#[cfg(target_os = "linux")]
pub fn who_holds(bus: u8, address: u8) -> Option<String> {
    let target = format!("/dev/bus/usb/{bus:03}/{address:03}");
    let me = std::process::id();
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        if pid == me {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else { continue };
        let holds = fds
            .flatten()
            .any(|fd| std::fs::read_link(fd.path()).map(|l| l.to_string_lossy() == target).unwrap_or(false));
        if holds {
            let name = std::fs::read_to_string(entry.path().join("comm"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "unknown".into());
            found.push(format!("{name} (pid {pid})"));
        }
    }
    if found.is_empty() {
        None
    } else {
        Some(found.join(", "))
    }
}

#[cfg(not(target_os = "linux"))]
pub fn who_holds(_bus: u8, _address: u8) -> Option<String> {
    None
}

/// True when the holder is the desktop's automatic camera mounting (GNOME gvfs).
pub fn is_desktop_service(holder: &str) -> bool {
    holder.contains("gvfs")
}

/// Stops the desktop's camera mounting until the next login, so the camera is free.
/// Only does anything on Linux desktops that use gvfs.
pub fn release_from_desktop_service() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let stop = Command::new("systemctl")
            .args([
                "--user",
                "stop",
                "gvfs-gphoto2-volume-monitor.service",
                "gvfs-mtp-volume-monitor.service",
            ])
            .status();
        let _ = Command::new("pkill").arg("gvfsd-gphoto2").status();
        let _ = Command::new("pkill").arg("gvfsd-mtp").status();
        match stop {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Could not stop the desktop camera service: {e}")),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("Not available on this system.".into())
    }
}
