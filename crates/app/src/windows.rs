//! Where the app's windows are on the screen, and snapping them together.
//!
//! "Windows" here are the main window and the panels shown as separate
//! windows. The operating system moves a window while the user drags it;
//! the app only sees that its position changed. From that it:
//!
//! - remembers each window's place, so it reopens there and saved layouts
//!   can put it back;
//! - when a drag stops near another window's edge, moves the window flush
//!   against it (snapping);
//! - when a window that touches others is dragged, moves those along
//!   (a group), unless the drag starts with a quick pull, which takes the
//!   one window out of its group.
//!
//! Groups are not stored: windows that touch form a group. Restoring
//! positions therefore restores groups too.
//!
//! This needs a system that tells apps where their windows are and lets
//! them move them: Windows and X11 do; Wayland does not, and there this
//! module does nothing.

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A window's position (of its outer frame) and the size of its contents.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A window's outer frame on the screen, for the snapping arithmetic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Frame {
    fn right(self) -> f32 {
        self.x + self.w
    }
    fn bottom(self) -> f32 {
        self.y + self.h
    }
    fn moved(self, dx: f32, dy: f32) -> Frame {
        Frame { x: self.x + dx, y: self.y + dy, ..self }
    }
}

/// How far two spans overlap; negative when they are apart.
fn overlap(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    a1.min(b1) - a0.max(b0)
}

/// True if the two frames sit edge to edge (within `slack` pixels) and
/// share part of that edge.
pub fn touching(a: Frame, b: Frame, slack: f32) -> bool {
    let side_by_side = ((a.right() - b.x).abs() <= slack || (b.right() - a.x).abs() <= slack)
        && overlap(a.y, a.bottom(), b.y, b.bottom()) > 0.0;
    let stacked = ((a.bottom() - b.y).abs() <= slack || (b.bottom() - a.y).abs() <= slack)
        && overlap(a.x, a.right(), b.x, b.right()) > 0.0;
    side_by_side || stacked
}

/// How far to move `moving` so it sits flush against the nearest edge of
/// one of `others`, if one is within `distance`. Sideways and up-down are
/// decided separately, so a window can snap into a corner. Once flush
/// against a window, its other edges also line up with that window's if
/// they are close.
pub fn snap_offset(moving: Frame, others: &[Frame], distance: f32) -> (f32, f32) {
    let mut best_x: Option<f32> = None;
    let mut best_y: Option<f32> = None;
    let consider = |best: &mut Option<f32>, offset: f32| {
        if offset.abs() <= distance && best.is_none_or(|b| offset.abs() < b.abs()) {
            *best = Some(offset);
        }
    };
    for other in others {
        // Near enough up-down for a side-by-side snap to make sense?
        let near_y = overlap(moving.y, moving.bottom(), other.y, other.bottom()) > -distance;
        let near_x = overlap(moving.x, moving.right(), other.x, other.right()) > -distance;
        if near_y {
            consider(&mut best_x, other.right() - moving.x); // our left to their right
            consider(&mut best_x, other.x - moving.right()); // our right to their left
        }
        if near_x {
            consider(&mut best_y, other.bottom() - moving.y);
            consider(&mut best_y, other.y - moving.bottom());
        }
    }
    // Line up the other direction with a neighbour we are (now) flush with.
    let placed = moving.moved(best_x.unwrap_or(0.0), best_y.unwrap_or(0.0));
    for other in others {
        if !touching(placed, *other, 1.0) {
            continue;
        }
        if best_y.is_none() {
            consider(&mut best_y, other.y - moving.y);
            consider(&mut best_y, other.bottom() - moving.bottom());
        }
        if best_x.is_none() {
            consider(&mut best_x, other.x - moving.x);
            consider(&mut best_x, other.right() - moving.right());
        }
    }
    (best_x.unwrap_or(0.0), best_y.unwrap_or(0.0))
}

/// The largest rectangle inside `area` that none of `obstacles` covers.
/// Used to maximise the main window without hiding the panel windows.
pub fn largest_free(area: Frame, obstacles: &[Frame]) -> Frame {
    // Only obstacles that reach into the area matter.
    let inside: Vec<Frame> = obstacles
        .iter()
        .copied()
        .filter(|o| overlap(o.x, o.right(), area.x, area.right()) > 0.0 && overlap(o.y, o.bottom(), area.y, area.bottom()) > 0.0)
        .collect();
    // The best rectangle has each side on the area's edge or an obstacle's.
    let mut xs = vec![area.x, area.right()];
    let mut ys = vec![area.y, area.bottom()];
    for o in &inside {
        xs.extend([o.x.clamp(area.x, area.right()), o.right().clamp(area.x, area.right())]);
        ys.extend([o.y.clamp(area.y, area.bottom()), o.bottom().clamp(area.y, area.bottom())]);
    }
    let mut best = Frame { x: area.x, y: area.y, w: 0.0, h: 0.0 };
    for &x0 in &xs {
        for &x1 in &xs {
            for &y0 in &ys {
                for &y1 in &ys {
                    if x1 <= x0 || y1 <= y0 || (x1 - x0) * (y1 - y0) <= best.w * best.h {
                        continue;
                    }
                    let free = inside
                        .iter()
                        .all(|o| overlap(o.x, o.right(), x0, x1) <= 0.0 || overlap(o.y, o.bottom(), y0, y1) <= 0.0);
                    if free {
                        best = Frame { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
                    }
                }
            }
        }
    }
    best
}

/// Everything that touches `start`, directly or through other windows.
pub fn group_of<K: Copy + PartialEq>(start: K, frames: &[(K, Frame)]) -> Vec<K> {
    let mut group = vec![start];
    let mut index = 0;
    while index < group.len() {
        let Some((_, here)) = frames.iter().find(|(k, _)| *k == group[index]) else { break };
        for (key, frame) in frames {
            if !group.contains(key) && touching(*here, *frame, 1.5) {
                group.push(*key);
            }
        }
        index += 1;
    }
    group
}

/// A drag is judged quick or slow over this long from its start…
const JUDGE: Duration = Duration::from_millis(150);
/// …and is quick if the window has travelled at least this far by then.
const QUICK_DISTANCE: f32 = 150.0;
/// Where the mouse button cannot be read, a drag counts as finished when
/// the window has stood still this long.
const LONG_SETTLE: Duration = Duration::from_millis(700);
/// After the app has moved windows, it ignores position changes this long:
/// windows arrive at their new places a little late, and those arrivals
/// must not be mistaken for the user dragging them.
const QUIET: Duration = Duration::from_millis(350);

/// A drag in progress.
struct Drag<K> {
    window: K,
    started: Instant,
    last_change: Instant,
    /// Where the dragged window was when the drag began.
    origin: Frame,
    /// The other windows of its group, with where each was at the start.
    companions: Vec<(K, Frame)>,
    /// `None` until the drag has been judged quick (pull free) or slow.
    carries_group: Option<bool>,
    /// The offset the companions were last sent to.
    sent: (f32, f32),
}

/// Follows the windows' positions from frame to frame and decides what to move.
///
/// The rules that keep it from fighting the system or itself:
/// - While the mouse button is down, only the companions of the dragged
///   window are moved. The dragged window itself is never touched: the
///   system is moving it. Snapping happens once, after the button is released.
/// - Only the dragged window's movement is read during a drag; the
///   companions' own movement is the app's doing and is ignored.
/// - After the app moves windows it looks away for a moment ([`QUIET`]).
/// - If a window ends up somewhere other than asked (some systems add the
///   title bar's height), the difference is learned and allowed for.
pub struct Snapper<K> {
    last: HashMap<K, Frame>,
    drag: Option<Drag<K>>,
    quiet_until: Option<Instant>,
    /// Where windows were asked to go at the end of the last drag, to check
    /// that they got there; and how many more times to correct them.
    asked: Vec<(K, f32, f32)>,
    corrections_left: u8,
    /// Per window: how far it lands from where it is asked to go.
    offset: HashMap<K, (f32, f32)>,
}

impl<K> Default for Snapper<K> {
    fn default() -> Self {
        Snapper {
            last: HashMap::new(),
            drag: None,
            quiet_until: None,
            asked: Vec::new(),
            corrections_left: 0,
            offset: HashMap::new(),
        }
    }
}

impl<K: Copy + PartialEq + Eq + std::hash::Hash> Snapper<K> {
    /// Forget everything, for example after windows were placed from a saved layout.
    pub fn reset(&mut self) {
        let offset = std::mem::take(&mut self.offset);
        *self = Snapper { offset, ..Snapper::default() };
    }

    /// The app is about to move windows itself: look away for a moment.
    pub fn look_away(&mut self, now: Instant) {
        self.drag = None;
        self.asked.clear();
        self.quiet_until = Some(now + QUIET * 2);
    }

    /// True while a drag is being followed or windows are settling, so the
    /// caller keeps checking.
    pub fn busy(&self) -> bool {
        self.drag.is_some() || self.quiet_until.is_some()
    }

    /// The positions to ask the system for, allowing for each window's
    /// learned landing offset.
    fn requests(&self, targets: &[(K, f32, f32)]) -> Vec<(K, f32, f32)> {
        targets
            .iter()
            .map(|(key, x, y)| {
                let (ox, oy) = self.offset.get(key).copied().unwrap_or((0.0, 0.0));
                (*key, x - ox, y - oy)
            })
            .collect()
    }

    /// Given where every window is now, returns the windows to move and
    /// where to (top-left corner of the outer frame). `button` says whether
    /// the left mouse button is held down anywhere on the screen, or `None`
    /// if that cannot be read.
    pub fn update(
        &mut self,
        now: Instant,
        frames: &[(K, Frame)],
        button: Option<bool>,
        enabled: bool,
        distance: f32,
    ) -> Vec<(K, f32, f32)> {
        if !enabled {
            self.reset();
            return Vec::new();
        }
        let find = |key: K| frames.iter().find(|(k, _)| *k == key).map(|(_, f)| *f);

        // Looking away after moving windows.
        if let Some(until) = self.quiet_until {
            if now < until {
                self.last = frames.iter().copied().collect();
                return Vec::new();
            }
            self.quiet_until = None;
            // Did the windows land where they were asked to? If not, learn
            // by how much they miss and ask once more.
            if self.corrections_left > 0 {
                self.corrections_left -= 1;
                let mut again = Vec::new();
                for (key, x, y) in std::mem::take(&mut self.asked) {
                    let Some(frame) = find(key) else { continue };
                    let (ex, ey) = (frame.x - x, frame.y - y);
                    let miss = ex.abs().max(ey.abs());
                    if miss > 0.5 && miss < 120.0 {
                        let entry = self.offset.entry(key).or_insert((0.0, 0.0));
                        entry.0 += ex;
                        entry.1 += ey;
                        again.push((key, x, y));
                    }
                }
                if !again.is_empty() {
                    let requests = self.requests(&again);
                    self.asked = again;
                    self.quiet_until = Some(now + QUIET);
                    self.last = frames.iter().copied().collect();
                    return requests;
                }
            }
            self.asked.clear();
        }

        let mut targets: Vec<(K, f32, f32)> = Vec::new();
        if let Some(drag) = &mut self.drag {
            let Some(frame) = find(drag.window) else {
                self.drag = None;
                self.last = frames.iter().copied().collect();
                return Vec::new();
            };
            if self.last.get(&drag.window).is_some_and(|b| (b.x, b.y) != (frame.x, frame.y)) {
                drag.last_change = now;
            }
            let (dx, dy) = (frame.x - drag.origin.x, frame.y - drag.origin.y);
            let ended = match button {
                Some(down) => !down,
                None => now.duration_since(drag.last_change) >= LONG_SETTLE,
            };
            // A quick pull at the start takes the window out of its group.
            if drag.carries_group.is_none() && (ended || now.duration_since(drag.started) >= JUDGE) {
                drag.carries_group = Some((dx * dx + dy * dy).sqrt() < QUICK_DISTANCE);
            }
            let carried = drag.carries_group == Some(true);
            if !ended {
                // The companions follow. The dragged window is left alone.
                if carried && drag.sent != (dx, dy) {
                    drag.sent = (dx, dy);
                    for (key, start) in &drag.companions {
                        targets.push((*key, start.x + dx, start.y + dy));
                    }
                }
            } else {
                // Released: snap the window (with its group) to the nearest
                // edge of a window outside the group. This is the only time
                // the dragged window itself is moved.
                let drag = self.drag.take().unwrap();
                let in_group = |k: &K| *k == drag.window || (carried && drag.companions.iter().any(|(c, _)| c == k));
                let others: Vec<Frame> = frames.iter().filter(|(k, _)| !in_group(k)).map(|(_, f)| *f).collect();
                let (sx, sy) = snap_offset(frame, &others, distance);
                if (sx, sy) != (0.0, 0.0) {
                    targets.push((drag.window, frame.x + sx, frame.y + sy));
                }
                if carried {
                    for (key, start) in &drag.companions {
                        targets.push((*key, start.x + dx + sx, start.y + dy + sy));
                    }
                }
                if !targets.is_empty() {
                    self.asked = targets.clone();
                    self.corrections_left = 2;
                    self.quiet_until = Some(now + QUIET);
                }
            }
        } else if button != Some(false) {
            // Has the user started dragging a window? Only a move with the
            // mouse button down counts (a resize is not a drag).
            let started = frames.iter().find(|(key, frame)| {
                self.last.get(key).is_some_and(|before| {
                    (frame.x, frame.y) != (before.x, before.y) && (frame.w, frame.h) == (before.w, before.h)
                })
            });
            if let Some((window, frame)) = started {
                // Note the group as it stood before the move.
                let before: Vec<(K, Frame)> =
                    frames.iter().map(|(k, f)| (*k, self.last.get(k).copied().unwrap_or(*f))).collect();
                let origin = before.iter().find(|(k, _)| k == window).map(|(_, f)| *f).unwrap_or(*frame);
                let companions = group_of(*window, &before)
                    .into_iter()
                    .filter(|k| k != window)
                    .filter_map(|k| before.iter().find(|(key, _)| *key == k).copied())
                    .collect();
                self.drag = Some(Drag {
                    window: *window,
                    started: now,
                    last_change: now,
                    origin,
                    companions,
                    carries_group: None,
                    sent: (0.0, 0.0),
                });
            }
        }

        self.last = frames.iter().copied().collect();
        self.requests(&targets)
    }
}

/// A window's place as the system reports it, if it does.
pub fn place_of(ctx: &egui::Context, id: egui::ViewportId) -> Option<Place> {
    ctx.input_for(id, |i| {
        let outer = i.viewport().outer_rect?;
        let inner = i.viewport().inner_rect?;
        Some(Place { x: outer.min.x, y: outer.min.y, width: inner.width(), height: inner.height() })
    })
}

/// A window's outer frame as the system reports it.
pub fn frame_of(ctx: &egui::Context, id: egui::ViewportId) -> Option<Frame> {
    ctx.input_for(id, |i| {
        let outer = i.viewport().outer_rect?;
        Some(Frame { x: outer.min.x, y: outer.min.y, w: outer.width(), h: outer.height() })
    })
}

/// Asks the system to put a window somewhere.
pub fn move_to(ctx: &egui::Context, id: egui::ViewportId, x: f32, y: f32) {
    ctx.send_viewport_cmd_to(id, egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: f32, y: f32, w: f32, h: f32) -> Frame {
        Frame { x, y, w, h }
    }

    #[test]
    fn a_window_snaps_flush_to_a_nearby_edge_and_lines_up() {
        let main = frame(100.0, 100.0, 800.0, 600.0);
        // 12 px right of the main window, 9 px lower than its top.
        let (dx, dy) = snap_offset(frame(912.0, 109.0, 300.0, 400.0), &[main], 20.0);
        assert_eq!((dx, dy), (-12.0, -9.0));
        // Too far away: stays.
        assert_eq!(snap_offset(frame(960.0, 109.0, 300.0, 400.0), &[main], 20.0), (0.0, 0.0));
        // Level with it but far below: an edge that is not beside us does not pull.
        assert_eq!(snap_offset(frame(905.0, 900.0, 300.0, 400.0), &[main], 20.0), (0.0, 0.0));
        // Underneath, overlapping its bottom edge a little.
        let (dx, dy) = snap_offset(frame(300.0, 690.0, 300.0, 200.0), &[main], 20.0);
        assert_eq!((dx, dy), (0.0, 10.0));
    }

    #[test]
    fn maximising_leaves_room_for_panel_windows() {
        let screen = frame(0.0, 0.0, 1920.0, 1080.0);
        // Nothing in the way: the whole screen.
        assert_eq!(largest_free(screen, &[]), screen);
        // A panel down the right side: everything to its left.
        let editor = frame(1520.0, 0.0, 400.0, 1080.0);
        assert_eq!(largest_free(screen, &[editor]), frame(0.0, 0.0, 1520.0, 1080.0));
        // A panel on the right and a strip along the bottom.
        let log = frame(0.0, 880.0, 1520.0, 200.0);
        assert_eq!(largest_free(screen, &[editor, log]), frame(0.0, 0.0, 1520.0, 880.0));
        // A small panel in a corner: the larger of the two remaining blocks.
        let small = frame(1620.0, 780.0, 300.0, 300.0);
        assert_eq!(largest_free(screen, &[small]), frame(0.0, 0.0, 1620.0, 1080.0));
        // A window on another monitor does not count.
        assert_eq!(largest_free(screen, &[frame(2000.0, 0.0, 500.0, 500.0)]), screen);
    }

    #[test]
    fn touching_windows_form_a_group() {
        let frames = [
            ("main", frame(0.0, 0.0, 800.0, 600.0)),
            ("editor", frame(800.0, 0.0, 300.0, 400.0)),
            ("log", frame(800.0, 400.0, 300.0, 200.0)),
            ("far", frame(1500.0, 0.0, 300.0, 300.0)),
            // Corner to corner only: not a shared edge.
            ("corner", frame(1100.0, 600.0, 100.0, 100.0)),
        ];
        let mut group = group_of("log", &frames);
        group.sort();
        assert_eq!(group, vec!["editor", "log", "main"]);
        assert_eq!(group_of("far", &frames), vec!["far"]);
    }

    /// A pretend screen: windows arrive where they are sent two frames
    /// late, and `lands_off` is added to wherever a window is sent (as
    /// some systems do).
    struct Screen {
        frames: Vec<(&'static str, Frame)>,
        snapper: Snapper<&'static str>,
        now: Instant,
        in_transit: Vec<(u8, &'static str, f32, f32)>,
        lands_off: (f32, f32),
        /// `None`: the mouse button cannot be read.
        can_read_button: bool,
    }

    impl Screen {
        fn new(frames: Vec<(&'static str, Frame)>) -> Self {
            let mut screen = Screen {
                frames,
                snapper: Snapper::default(),
                now: Instant::now(),
                in_transit: Vec::new(),
                lands_off: (0.0, 0.0),
                can_read_button: true,
            };
            screen.frame(false);
            screen
        }

        /// One frame of the app: 16 ms pass, windows in transit arrive,
        /// the snapper looks and its requests set off.
        fn frame(&mut self, button_down: bool) {
            self.now += Duration::from_millis(16);
            let mut arrived = Vec::new();
            self.in_transit.retain_mut(|(wait, key, x, y)| {
                if *wait == 0 {
                    arrived.push((*key, *x, *y));
                    false
                } else {
                    *wait -= 1;
                    true
                }
            });
            for (key, x, y) in arrived {
                let f = &mut self.frames.iter_mut().find(|(k, _)| *k == key).unwrap().1;
                f.x = x + self.lands_off.0;
                f.y = y + self.lands_off.1;
            }
            let button = self.can_read_button.then_some(button_down);
            for (key, x, y) in self.snapper.update(self.now, &self.frames, button, true, 20.0) {
                self.in_transit.push((2, key, x, y));
            }
        }

        /// The user drags `window` by `step` pixels a frame, then lets go
        /// and waits.
        fn drag(&mut self, window: &'static str, step: (f32, f32), steps: usize) {
            for _ in 0..steps {
                let f = &mut self.frames.iter_mut().find(|(k, _)| *k == window).unwrap().1;
                *f = f.moved(step.0, step.1);
                // The system keeps the dragged window under the mouse,
                // whatever the app asks for.
                self.in_transit.retain(|(_, key, _, _)| *key != window);
                self.frame(true);
            }
            for _ in 0..120 {
                self.frame(false);
            }
        }

        fn at(&self, key: &str) -> (f32, f32) {
            let f = self.frames.iter().find(|(k, _)| *k == key).unwrap().1;
            (f.x, f.y)
        }
    }

    #[test]
    fn a_slow_drag_carries_the_group_and_a_quick_pull_frees_one_window() {
        let mut screen = Screen::new(vec![
            ("main", frame(0.0, 0.0, 800.0, 600.0)),
            ("editor", frame(800.0, 0.0, 300.0, 400.0)),
            ("far", frame(2000.0, 900.0, 300.0, 300.0)),
        ]);
        // Slowly: 3 px a frame. The editor follows the main window.
        screen.drag("main", (3.0, 2.0), 40);
        assert_eq!(screen.at("main"), (120.0, 80.0));
        assert_eq!(screen.at("editor"), (920.0, 80.0));
        assert_eq!(screen.at("far"), (2000.0, 900.0));

        // A quick pull: 30 px a frame. The editor leaves; main stays.
        screen.drag("editor", (30.0, 10.0), 12);
        assert_eq!(screen.at("main"), (120.0, 80.0));
        assert_eq!(screen.at("editor"), (1280.0, 200.0));
    }

    #[test]
    fn nothing_is_moved_while_the_button_is_down_except_companions() {
        let mut screen =
            Screen::new(vec![("main", frame(0.0, 0.0, 800.0, 600.0)), ("slots", frame(1200.0, 300.0, 300.0, 300.0))]);
        // Approach slowly and hover 10 px from the edge for two seconds,
        // button still down: the window must stay exactly under the mouse.
        for _ in 0..130 {
            let f = &mut screen.frames[1].1;
            *f = f.moved(-3.0, 0.0);
            screen.frame(true);
            assert!(screen.in_transit.is_empty(), "no window may be moved during the approach");
        }
        for _ in 0..125 {
            screen.frame(true);
            assert!(screen.in_transit.is_empty(), "holding still with the button down must not snap");
        }
        assert_eq!(screen.at("slots"), (810.0, 300.0));
        // Released: it snaps once, flush, and stays.
        for _ in 0..120 {
            screen.frame(false);
        }
        assert_eq!(screen.at("slots"), (800.0, 300.0));
        assert_eq!(screen.at("main"), (0.0, 0.0));
        // Now moving the main window brings it along.
        screen.drag("main", (2.0, 0.0), 30);
        assert_eq!(screen.at("main"), (60.0, 0.0));
        assert_eq!(screen.at("slots"), (860.0, 300.0));
    }

    #[test]
    fn windows_that_land_off_target_are_corrected_not_chased() {
        // This system puts a window 37 px below where it is asked to go.
        let mut screen =
            Screen::new(vec![("main", frame(0.0, 0.0, 800.0, 600.0)), ("slots", frame(1200.0, 300.0, 300.0, 300.0))]);
        screen.lands_off = (0.0, 37.0);
        screen.drag("slots", (-3.0, 0.0), 130);
        assert_eq!(screen.at("slots"), (800.0, 300.0), "flush, and at the height it was dropped");
        // The group still moves as one, and everything comes to rest.
        screen.drag("main", (2.0, 1.0), 30);
        assert_eq!(screen.at("main"), (60.0, 30.0));
        assert_eq!(screen.at("slots"), (860.0, 330.0));
        assert!(screen.in_transit.is_empty() && !screen.snapper.busy());
    }

    #[test]
    fn without_a_readable_button_it_waits_for_a_long_pause() {
        let mut screen =
            Screen::new(vec![("main", frame(0.0, 0.0, 800.0, 600.0)), ("slots", frame(1200.0, 300.0, 300.0, 300.0))]);
        screen.can_read_button = false;
        for _ in 0..130 {
            let f = &mut screen.frames[1].1;
            *f = f.moved(-3.0, 0.0);
            screen.frame(true);
        }
        // A short pause (300 ms) is not the end of the drag.
        for _ in 0..19 {
            screen.frame(true);
        }
        assert_eq!(screen.at("slots"), (810.0, 300.0));
        for _ in 0..120 {
            screen.frame(false);
        }
        assert_eq!(screen.at("slots"), (800.0, 300.0));
    }

    #[test]
    fn switched_off_it_does_nothing() {
        let mut frames = vec![("main", frame(0.0, 0.0, 800.0, 600.0)), ("editor", frame(800.0, 0.0, 300.0, 400.0))];
        let mut snapper = Snapper::default();
        let now = Instant::now();
        snapper.update(now, &frames, Some(false), false, 20.0);
        frames[0].1 = frames[0].1.moved(50.0, 0.0);
        assert!(snapper.update(now + Duration::from_millis(16), &frames, Some(true), false, 20.0).is_empty());
    }
}
