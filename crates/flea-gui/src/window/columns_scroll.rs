//! Each track retains the shared kinetic policy, the notch glide and the bar's reveal state.
use crate::input::{
    scroll::{is_touchpad, Axis, Limits, Wheel, TOUCH_GAIN},
    Frame, Glide, PaneInput,
};
use gpui::{ScrollDelta, ScrollWheelEvent, TouchPhase};
use std::{rc::Rc, time::Instant};

#[derive(Default)]
pub(crate) struct Scrolling {
    pub input: PaneInput,
    path: String,
    epoch: Option<Instant>,
    snapshot: usize,
}

impl crate::state::Shell {
    pub(crate) fn advance_columns_scroll(
        &mut self,
        side: usize,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let blocked = self.wheel_blocked(cx);
        let now = cx.background_executor().now();
        let reduced = cx.reduce_motion();
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        let Some(columns) = &mut host.columns else {
            return;
        };
        let tracks = Rc::clone(columns.area.tracks());
        let mut writes = [None; 5];
        let mut released = false;
        for (index, state) in columns.scroll.iter_mut().enumerate() {
            if !tracks.iter().any(|view| view.slot.index() == index) {
                released |= state.input.grab.is_some();
                *state = Scrolling::default();
            }
        }
        for view in tracks.iter() {
            let state = &mut columns.scroll[view.slot.index()];
            if blocked
                || host.pane.busy()
                || matches!(view.rows, crate::columns::TrackRows::Preview)
            {
                state.input.halt();
                state.input.stroke = false;
                released |= state.input.grab.take().is_some();
                continue;
            }
            let Some(mut frame) = state.input.frame else {
                continue;
            };
            frame.scroll = if view.slot == crate::columns::geometry::Slot::Active {
                host.pane.scroll_y
            } else {
                columns.area.scroll(view.slot)
            };
            let total = match &view.rows {
                crate::columns::TrackRows::Listing => host
                    .filter
                    .shown
                    .as_ref()
                    .map_or(host.pane.held.total, |rows| rows.len()),
                crate::columns::TrackRows::Peek(rows) => rows.len(),
                _ => 0,
            };
            let snapshot = snapshot(
                &view.rows,
                host.filter.shown.as_ref().map(|rows| rows.as_ptr()),
            );
            if total != frame.total || view.path != state.path || snapshot != state.snapshot {
                released |= state.input.grab.is_some();
                *state = Scrolling::default();
                continue;
            }
            if view.slot == crate::columns::geometry::Slot::Active {
                frame.listing = host.pane.held.listing
            }
            let grabbed = state.input.grab.is_some();
            state.prepare(&view.path, frame, snapshot);
            released |= grabbed && state.input.grab.is_none();
            if let Some(y) = state.advance(now, reduced) {
                writes[view.slot.index()] = Some((view.slot, y));
            }
            if state.input.moving() {
                window.request_animation_frame()
            }
        }
        for (slot, y) in writes.into_iter().flatten() {
            super::scroll_input::write(host, slot, y)
        }
        if released {
            self.hold_pointer(side, false, window, cx)
        }
    }
}

impl Scrolling {
    pub fn prepare(&mut self, path: &str, frame: Frame, snapshot: usize) {
        if self.path != path
            || self.snapshot != snapshot
            || self.input.frame.is_some_and(|old| {
                old.listing != frame.listing
                    || old.total != frame.total
                    || old.bounds != frame.bounds
                    || old.lane != frame.lane
            })
        {
            *self = Self::default();
            self.path = path.into();
            self.snapshot = snapshot;
        }
        if self.input.wrote.is_some_and(|wrote| wrote != frame.scroll) {
            self.input.halt();
        }
        self.input.frame = Some(frame);
    }

    fn axis(&self) -> Option<Axis> {
        let frame = self.input.frame?;
        Some(Axis {
            at: f64::from(frame.scroll + self.input.over),
            limits: Limits {
                min: 0.,
                max: f64::from(frame.max_scroll()),
            },
            size: f64::from(frame.viewport()),
            interactive: frame.total > 0,
        })
    }

    fn apply(&mut self, axis: Axis) -> f32 {
        let frame = self.input.frame.as_mut().expect("prepared track");
        frame.scroll = (axis.at as f32).clamp(0., frame.max_scroll());
        self.input.over = axis.at as f32 - frame.scroll;
        self.input.wrote = Some(frame.scroll);
        frame.scroll
    }

    pub fn wheel(&mut self, event: &ScrollWheelEvent, now: Instant, reduced: bool) -> bool {
        let in_stroke = self.input.stroke || event.touch_phase != TouchPhase::Moved;
        self.input.stroke = match event.touch_phase {
            TouchPhase::Started => true,
            TouchPhase::Ended | TouchPhase::Cancelled => false,
            TouchPhase::Moved => self.input.stroke,
        };
        let Some(mut axis) = self.axis() else {
            return false;
        };
        let Some(mut wheel) = crate::input::wheel_delivery(event, in_stroke) else {
            self.input.kinetic.press(&mut axis);
            self.input.kinetic.start_return(&mut axis, true);
            self.input.glide = None;
            self.apply(axis);
            return true;
        };
        // Columns touchpad pixels pass through ungained, as v0.3.12's FastScrollHandler moves them.
        if let ScrollDelta::Pixels(pixels) = event.delta {
            let y = f64::from(f32::from(pixels.y));
            wheel = if is_touchpad(wheel.phase) {
                Wheel {
                    pixel: y / TOUCH_GAIN,
                    ..wheel
                }
            } else {
                Wheel {
                    pixel: y,
                    ..Wheel::default()
                }
            };
        }
        let before = axis.at;
        let frame = self.input.frame.unwrap();
        let from = self.input.drawn(frame.scroll);
        let epoch = *self.epoch.get_or_insert(now);
        let at = now.saturating_duration_since(epoch).as_secs_f64() * 1000.;
        let consumed = self.input.kinetic.wheel(&mut axis, wheel, at, reduced);
        let to = self.apply(axis);
        if wheel.angle != 0. && !reduced && before != axis.at {
            self.input.glide = Some(Glide {
                from,
                to,
                elapsed: 0.,
            });
        }
        if before != axis.at {
            self.input.moved(now);
        }
        self.input.motion_at.get_or_insert(now);
        consumed
    }

    pub fn advance(&mut self, now: Instant, reduced: bool) -> Option<f32> {
        let mut axis = self.axis()?;
        let dt = self.input.motion_at.map_or(0., |at| {
            now.saturating_duration_since(at).as_secs_f64() * 1000.
        });
        let before = axis.at;
        self.input.kinetic.advance(&mut axis, dt, reduced);
        let to = self.apply(axis);
        if before != axis.at {
            self.input.moved(now);
        }
        if let Some(glide) = &mut self.input.glide {
            glide.elapsed += dt as f32;
            if reduced || f64::from(glide.elapsed) >= crate::input::scroll::RETURN_MS {
                self.input.glide = None;
            }
        }
        self.input.motion_at = self.input.moving().then_some(now);
        Some(to)
    }

    pub fn hold(&mut self) -> Option<f32> {
        let frame = self.input.frame?;
        let to = self.input.drawn(frame.scroll);
        self.input.glide = None;
        let mut axis = self.axis()?;
        axis.at = f64::from(to);
        self.input.kinetic.press(&mut axis);
        Some(self.apply(axis))
    }

    pub fn release(&mut self, now: Instant, reduced: bool) {
        let Some(mut axis) = self.axis() else { return };
        if self.input.kinetic.overscrolled(&axis) {
            self.input.kinetic.start_return(&mut axis, reduced);
            self.apply(axis);
            self.input.motion_at = Some(now);
        }
    }
}

pub(super) fn snapshot(rows: &crate::columns::TrackRows, filter: Option<*const usize>) -> usize {
    match rows {
        crate::columns::TrackRows::Peek(rows) => Rc::as_ptr(rows) as *const () as usize,
        crate::columns::TrackRows::Listing => filter.map_or(0, |rows| rows as usize),
        _ => 0,
    }
}
