//! Two questions for the system: is the left mouse button held down,
//! anywhere on the screen? And how large is the usable part of a monitor?
//!
//! While the user drags a window by its title bar, the system keeps the
//! mouse to itself and the app hears nothing about it. Window snapping
//! must wait until the button is released, so the app asks the system
//! directly. On Linux that is a question to the X server; elsewhere (and
//! if the X server cannot be reached) the answer is "unknown", and
//! snapping falls back to waiting for the window to stand still.

pub struct Buttons {
    #[cfg(target_os = "linux")]
    x11: Option<(x11rb::rust_connection::RustConnection, u32)>,
}

impl Buttons {
    pub fn new() -> Self {
        #[cfg(target_os = "linux")]
        {
            let x11 = x11rb::connect(None).ok().and_then(|(connection, screen)| {
                use x11rb::connection::Connection;
                let root = connection.setup().roots.get(screen)?.root;
                Some((connection, root))
            });
            Buttons { x11 }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Buttons {}
        }
    }

    /// The usable area (in screen pixels: x, y, width, height) of the
    /// monitor that contains the point: the monitor, less the desktop's
    /// own bars and docks where the desktop reports them. `None` if unknown.
    pub fn work_area(&self, x: f32, y: f32) -> Option<(f32, f32, f32, f32)> {
        #[cfg(target_os = "linux")]
        {
            use x11rb::protocol::randr::ConnectionExt as _;
            use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};
            let (connection, root) = self.x11.as_ref()?;
            let monitors = connection.randr_get_monitors(*root, true).ok()?.reply().ok()?.monitors;
            let all: Vec<(f32, f32, f32, f32)> =
                monitors.iter().map(|m| (m.x as f32, m.y as f32, m.width as f32, m.height as f32)).collect();
            let (mx, my, mw, mh) = monitor_at(&all, x, y)?;
            let (mut x0, mut y0) = (mx, my);
            let (mut x1, mut y1) = (mx + mw, my + mh);
            // The desktop's work area spans all monitors; keep its part of this one.
            let work = connection
                .intern_atom(false, b"_NET_WORKAREA")
                .ok()
                .and_then(|c| c.reply().ok())
                .and_then(|atom| connection.get_property(false, *root, atom.atom, AtomEnum::CARDINAL, 0, 4).ok())
                .and_then(|c| c.reply().ok())
                .and_then(|reply| reply.value32().map(|v| v.collect::<Vec<u32>>()));
            if let Some(&[wx, wy, ww, wh]) = work.as_deref() {
                let (wx, wy, ww, wh) = (wx as f32, wy as f32, ww as f32, wh as f32);
                let (nx0, ny0, nx1, ny1) = (x0.max(wx), y0.max(wy), x1.min(wx + ww), y1.min(wy + wh));
                if nx1 - nx0 > 200.0 && ny1 - ny0 > 200.0 {
                    (x0, y0, x1, y1) = (nx0, ny0, nx1, ny1);
                }
            }
            Some((x0, y0, x1 - x0, y1 - y0))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (x, y);
            None
        }
    }

    /// Where the mouse pointer is, in screen pixels. `None` if unknown.
    pub fn pointer_position(&self) -> Option<(f32, f32)> {
        #[cfg(target_os = "linux")]
        {
            use x11rb::protocol::xproto::ConnectionExt;
            let (connection, root) = self.x11.as_ref()?;
            let reply = connection.query_pointer(*root).ok()?.reply().ok()?;
            Some((reply.root_x as f32, reply.root_y as f32))
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// `Some(true)` while the left button is down, `None` if unknown.
    pub fn left_down(&self) -> Option<bool> {
        #[cfg(target_os = "linux")]
        {
            use x11rb::protocol::xproto::{ConnectionExt, KeyButMask};
            let (connection, root) = self.x11.as_ref()?;
            let reply = connection.query_pointer(*root).ok()?.reply().ok()?;
            Some(u16::from(reply.mask) & u16::from(KeyButMask::BUTTON1) != 0)
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }
}

/// The monitor (x, y, width, height) that contains the point; if none
/// does, the nearest one.
pub fn monitor_at(monitors: &[(f32, f32, f32, f32)], x: f32, y: f32) -> Option<(f32, f32, f32, f32)> {
    let distance = |&(mx, my, mw, mh): &(f32, f32, f32, f32)| {
        let dx = (mx - x).max(x - (mx + mw)).max(0.0);
        let dy = (my - y).max(y - (my + mh)).max(0.0);
        dx * dx + dy * dy
    };
    let contains = |&&(mx, my, mw, mh): &&(f32, f32, f32, f32)| x >= mx && x < mx + mw && y >= my && y < my + mh;
    monitors.iter().find(contains).copied().or_else(|| monitors.iter().copied().min_by(|a, b| distance(a).total_cmp(&distance(b))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_monitor_under_the_pointer_is_chosen() {
        let left = (0.0, 0.0, 2560.0, 1440.0);
        let right = (2560.0, 0.0, 1920.0, 1080.0);
        let monitors = [left, right];
        assert_eq!(monitor_at(&monitors, 300.0, 500.0), Some(left));
        assert_eq!(monitor_at(&monitors, 3000.0, 500.0), Some(right));
        assert_eq!(monitor_at(&monitors, 2559.0, 10.0), Some(left));
        assert_eq!(monitor_at(&monitors, 2560.0, 10.0), Some(right));
        // Below the smaller monitor (no monitor there): the nearest.
        assert_eq!(monitor_at(&monitors, 3000.0, 1300.0), Some(right));
        assert_eq!(monitor_at(&[], 0.0, 0.0), None);
    }
}
