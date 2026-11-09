//! The text and diff viewport: its position, the wheel and lane inside the card, and the overlay knob drawn only on use.
use super::look::{self, Look};
use flea::ui_logic_preview_core::scroll::{self, Delta};
use gpui::{div, point, prelude::*, px, size, AnyElement, Bounds, Pixels, Point, ScrollDelta};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Vertical,
    Horizontal,
}

/// GPUI's Wayland client multiplies each touchpad axis value by 3.0 (gpui-pre-linux/src/linux/wayland/client.rs:2774 at af2dfcb); Qt's pixelDelta, which Scroll.js scales by 2.5, is the value itself.
pub const GPUI_TOUCHPAD_SCALE: f64 = 3.;

#[path = "scroller_capture.rs"]
pub(crate) mod capture;
#[path = "scroller_overlay.rs"]
pub(crate) mod overlay;

const AXES: [Axis; 2] = [Axis::Vertical, Axis::Horizontal];

/// One frame's viewport, set while the card draws: its window rect and the content under it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub bounds: Bounds<Pixels>,
    pub content_w: f64,
    pub content_h: f64,
    /// The diff pans sideways too and keeps both bars (ViewportScrollBars); the text card has the vertical bar alone.
    pub sideways: bool,
}

fn f(value: Pixels) -> f64 {
    f64::from(f32::from(value))
}

impl View {
    fn extent(&self, axis: Axis) -> (f64, f64) {
        match axis {
            Axis::Vertical => (self.content_h, f(self.bounds.size.height)),
            Axis::Horizontal => (self.content_w, f(self.bounds.size.width)),
        }
    }

    fn scrolls(&self, axis: Axis) -> bool {
        let (content, viewport) = self.extent(axis);
        (axis == Axis::Vertical || self.sideways) && scroll::overflows(content, viewport)
    }

    /// The lane in viewport coordinates; with two bars each stops one lane short of the shared corner.
    fn lane(&self, axis: Axis) -> Bounds<Pixels> {
        let (w, h) = (f(self.bounds.size.width), f(self.bounds.size.height));
        let lane = f64::from(look::PAD);
        let inset = if self.sideways { lane } else { 0. };
        let (x, y, lw, lh) = match axis {
            Axis::Vertical => (w - lane, 0., lane, h - inset),
            Axis::Horizontal => (0., h - lane, w - inset, lane),
        };
        Bounds::new(
            point(px(x as f32), px(y as f32)),
            size(px(lw.max(0.) as f32), px(lh.max(0.) as f32)),
        )
    }

    fn track(&self, axis: Axis) -> f64 {
        let lane = self.lane(axis).size;
        f(match axis {
            Axis::Vertical => lane.height,
            Axis::Horizontal => lane.width,
        })
    }

    fn local(&self, at: Point<Pixels>) -> Point<Pixels> {
        at - self.bounds.origin
    }

    fn along(axis: Axis, at: Point<Pixels>) -> f64 {
        f(match axis {
            Axis::Vertical => at.y,
            Axis::Horizontal => at.x,
        })
    }
}

/// Quick Look's one scroll position, reset with each subject.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scroll {
    pub y: f64,
    pub x: f64,
    /// Per axis: the view moved within the last HOLD_MS, so its bar shows.
    pub moving: [bool; 2],
    pub generation: [u64; 2],
    /// The lane under the pointer, which widens the knob and keeps it shown.
    pub lane: Option<Axis>,
    /// Both hover lanes include the shared corner, while their painted tracks stop short.
    pub corner: bool,
    /// A knob held by the pointer, and where on it the press landed.
    pub drag: Option<(Axis, f64)>,
    /// A zoomed picture held by the pointer: where the press landed and the position it started from.
    pub pan: Option<(Point<Pixels>, f64, f64)>,
    /// A zoom step's centre as fractions of the content, applied to the next frame's viewport.
    pub anchor: Option<(f64, f64)>,
    pub view: Option<View>,
    /// The diff's longest line in px, measured once per subject.
    pub widest: Option<f64>,
}

fn index(axis: Axis) -> usize {
    match axis {
        Axis::Vertical => 0,
        Axis::Horizontal => 1,
    }
}

impl Scroll {
    pub fn at(&self, axis: Axis) -> f64 {
        match axis {
            Axis::Vertical => self.y,
            Axis::Horizontal => self.x,
        }
    }

    fn set(&mut self, axis: Axis, value: f64) -> bool {
        let Some(view) = self.view else { return false };
        let (content, viewport) = view.extent(axis);
        let value = scroll::bounded(value, content, viewport);
        let slot = match axis {
            Axis::Vertical => &mut self.y,
            Axis::Horizontal => &mut self.x,
        };
        let moved = (value - *slot).abs() > 0.01;
        *slot = value;
        moved
    }

    /// This frame's viewport; a resize or an expand keeps the position inside the new bounds.
    pub fn fit(&mut self, view: View) {
        self.view = Some(view);
        if let Some((fx, fy)) = self.anchor.take() {
            self.x = fx * view.content_w - f(view.bounds.size.width) / 2.;
            self.y = fy * view.content_h - f(view.bounds.size.height) / 2.;
        }
        let (y, x) = (self.y, if view.sideways { self.x } else { 0. });
        self.set(Axis::Vertical, y);
        self.set(Axis::Horizontal, x);
    }

    /// A wheel or touchpad event; returns the axes it moved, whose bars then show for HOLD_MS.
    pub fn wheel(&mut self, at: Point<Pixels>, delta: ScrollDelta) -> Vec<Axis> {
        let Some(view) = self.view.filter(|v| v.bounds.contains(&at)) else {
            return Vec::new();
        };
        let (dx, dy) = match delta {
            ScrollDelta::Lines(p) => (Delta::Lines(f64::from(p.x)), Delta::Lines(f64::from(p.y))),
            ScrollDelta::Pixels(p) => (
                Delta::Pixels(f(p.x) / GPUI_TOUCHPAD_SCALE),
                Delta::Pixels(f(p.y) / GPUI_TOUCHPAD_SCALE),
            ),
        };
        let mut moved = Vec::new();
        for (axis, delta) in [(Axis::Vertical, dy), (Axis::Horizontal, dx)] {
            if view.scrolls(axis) && self.set(axis, self.at(axis) - scroll::distance(delta)) {
                moved.push(axis);
            }
        }
        moved
    }

    /// The pointer moved: the lane it is over, and a held knob follows it while the left button is down; returns whether anything changed.
    pub fn hover(&mut self, at: Point<Pixels>, held: bool) -> (bool, Option<Axis>) {
        let Some(view) = self.view else {
            return (false, None);
        };
        let local = view.local(at);
        let lane = AXES
            .into_iter()
            .find(|&a| view.scrolls(a) && view.lane(a).contains(&local));
        let corner = view.sideways
            && view.bounds.contains(&at)
            && local.x >= view.bounds.size.width - px(look::PAD)
            && local.y >= view.bounds.size.height - px(look::PAD);
        let mut changed = lane != self.lane || corner != self.corner;
        self.lane = lane;
        self.corner = corner;
        // A release the window never saw: the first move without the button ends the drag, as the lane's onCanceled does.
        if !held && (self.drag.take().is_some() | self.pan.take().is_some()) {
            changed = true;
        }
        let mut dragged = None;
        if let Some((axis, grab)) = self.drag {
            let (content, viewport) = view.extent(axis);
            let handle =
                View::along(axis, local) - View::along(axis, view.lane(axis).origin) - grab;
            let to = scroll::position_for_handle(handle, content, viewport, view.track(axis));
            if self.set(axis, to) {
                changed = true;
                dragged = Some(axis);
            }
        }
        (changed, dragged)
    }

    /// A left press: on a lane it jumps the knob's centre under the pointer if it missed, then holds the knob.
    pub fn press(&mut self, at: Point<Pixels>) -> Option<Axis> {
        let view = self.view?;
        let local = view.local(at);
        let axis = AXES
            .into_iter()
            .find(|&a| view.scrolls(a) && view.lane(a).contains(&local))?;
        let (content, viewport) = view.extent(axis);
        let track = view.track(axis);
        let on = View::along(axis, local) - View::along(axis, view.lane(axis).origin);
        let length = scroll::handle_length(track, content, viewport);
        let mut start = scroll::handle_offset(self.at(axis), content, viewport, track);
        if on < start || on > start + length {
            start = scroll::jump_offset(on, length, scroll::travel(track, content, viewport));
            self.set(
                axis,
                scroll::position_for_handle(start, content, viewport, track),
            );
        }
        self.drag = Some((axis, on - start));
        Some(axis)
    }

    pub fn release(&mut self) -> bool {
        let panned = self.pan.take().is_some();
        self.drag.take().is_some() || panned
    }

    /// The viewport point under the centre, as fractions of the content; a step not yet drawn keeps its own.
    pub fn centre(&self) -> (f64, f64) {
        self.anchor
            .or_else(|| {
                self.view.map(|view| {
                    (
                        (self.x + f(view.bounds.size.width) / 2.) / view.content_w,
                        (self.y + f(view.bounds.size.height) / 2.) / view.content_h,
                    )
                })
            })
            .unwrap_or((0.5, 0.5))
    }

    /// A left press on a viewport whose content overflows takes hold of the content, as a Flickable's drag does.
    pub fn grab(&mut self, at: Point<Pixels>) -> bool {
        let Some(view) = self.view.filter(|view| view.bounds.contains(&at)) else {
            return false;
        };
        if !AXES.into_iter().any(|axis| view.scrolls(axis)) {
            return false;
        }
        self.pan = Some((at, self.x, self.y));
        true
    }

    /// The held content follows the pointer; returns the axes it moved.
    pub fn pan_to(&mut self, at: Point<Pixels>) -> Vec<Axis> {
        let Some((from, x, y)) = self.pan else {
            return Vec::new();
        };
        let mut moved = Vec::new();
        for (axis, start) in [(Axis::Vertical, y), (Axis::Horizontal, x)] {
            let to = start - (View::along(axis, at) - View::along(axis, from));
            if self.set(axis, to) {
                moved.push(axis);
            }
        }
        moved
    }

    /// Marks an axis moving and returns the generation its hold timer must still match.
    pub fn moved(&mut self, axis: Axis) -> u64 {
        let i = index(axis);
        self.moving[i] = true;
        self.generation[i] += 1;
        self.generation[i]
    }

    /// The hold ran out; true when this was the latest move on that axis.
    pub fn settle(&mut self, axis: Axis, generation: u64) -> bool {
        let i = index(axis);
        let latest = self.generation[i] == generation && self.moving[i];
        if latest {
            self.moving[i] = false;
        }
        latest
    }

    /// Each bar shown on use: the track while the lane is used, the knob in the foreground at the scroller's alphas.
    pub fn bars(&self, look: &Look) -> Vec<AnyElement> {
        let Some(view) = self.view else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for axis in AXES {
            let pressed = self.drag.is_some_and(|(a, _)| a == axis);
            let in_lane = self.lane == Some(axis) || self.corner;
            let (content, viewport) = view.extent(axis);
            let shown = view.scrolls(axis)
                && scroll::revealed(true, self.moving[index(axis)], in_lane, pressed);
            if !shown {
                continue;
            }
            let lane = view.lane(axis);
            let track = view.track(axis);
            let wide = in_lane || pressed;
            let knob = if wide {
                scroll::KNOB_WIDE_PX
            } else {
                scroll::KNOB_REST_PX
            };
            let alpha = if pressed {
                scroll::KNOB_PRESSED_ALPHA
            } else if wide {
                scroll::KNOB_HOVER_ALPHA
            } else {
                scroll::KNOB_ALPHA
            };
            let length = scroll::handle_length(track, content, viewport);
            let offset = scroll::handle_offset(self.at(axis), content, viewport, track);
            let (lx, ly) = (f(lane.origin.x), f(lane.origin.y));
            let (lw, lh) = (f(lane.size.width), f(lane.size.height));
            let inset = scroll::KNOB_INSET_PX;
            let (x, y, w, h, selector) = match axis {
                Axis::Vertical => (
                    lx + lw - inset - knob,
                    ly + offset,
                    knob,
                    length,
                    "pv1-scroll-knob",
                ),
                Axis::Horizontal => (
                    lx + offset,
                    ly + lh - inset - knob,
                    length,
                    knob,
                    "pv1-scroll-knob-x",
                ),
            };
            if wide {
                out.push(
                    div()
                        .absolute()
                        .left(lane.origin.x)
                        .top(lane.origin.y)
                        .w(lane.size.width)
                        .h(lane.size.height)
                        .bg(look.foreground.opacity(scroll::TRACK_ALPHA))
                        .into_any_element(),
                );
            }
            let radius = scroll::knob_radius(f(look.corner), w, h);
            out.push(
                div()
                    .debug_selector(move || selector.into())
                    .absolute()
                    .left(px(x as f32))
                    .top(px(y as f32))
                    .w(px(w as f32))
                    .h(px(h as f32))
                    .rounded(px(radius as f32))
                    .bg(look.foreground.opacity(alpha))
                    .into_any_element(),
            );
        }
        out
    }
}
