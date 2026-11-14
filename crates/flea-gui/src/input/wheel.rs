//! Wheel delivery: a notch moves three lines of 24px times four and glides there on OutCubic; a touchpad stroke drags, rubber-bands past an end and leaves a tail.
use super::{
    scroll::{self, Axis, Limits, Phase, Wheel},
    Glide, PaneInput,
};
use crate::{list::pane::Pane, state::Shell, window::startup::View};
use gpui::{Context, ScrollDelta, ScrollWheelEvent, TouchPhase, Window};
use std::time::Instant;

#[cfg(test)]
#[path = "scroll_parity_tests.rs"]
mod parity_tests;

/// One native event in the shared scroll policy's units; cancellation is handled by the motion owner.
pub(crate) fn delivery(event: &ScrollWheelEvent, in_stroke: bool) -> Option<Wheel> {
    Some(match event.delta {
        ScrollDelta::Lines(lines) => Wheel {
            angle: f64::from(lines.y) / scroll::FORK_NOTCH_LINES * scroll::NOTCH_UNITS,
            ..Wheel::default()
        },
        ScrollDelta::Pixels(pixels) => {
            let raw = scroll::touchpad_raw(f64::from(f32::from(pixels.y)));
            let phase = match event.touch_phase {
                TouchPhase::Started => Phase::Begin,
                TouchPhase::Moved => Phase::Update,
                TouchPhase::Ended => Phase::End,
                TouchPhase::Cancelled => return None,
            };
            if in_stroke {
                Wheel {
                    phase,
                    pixel: raw,
                    ..Wheel::default()
                }
            } else {
                Wheel {
                    angle: scroll::continuous_angle(raw),
                    ..Wheel::default()
                }
            }
        }
    })
}

impl PaneInput {
    /// The axis the motion writes: the drawn position past an end included, bounded by this frame's geometry.
    fn axis(&self, scroll_y: f32) -> Option<Axis> {
        let frame = self.frame?;
        Some(Axis {
            at: f64::from(scroll_y + self.over),
            limits: Limits {
                min: 0.,
                max: f64::from(frame.max_scroll()),
            },
            size: f64::from(frame.viewport()),
            interactive: frame.total > 0,
        })
    }
    /// The pane keeps the clamped position; what a stroke drew past an end is the overshoot.
    fn apply_axis(&mut self, pane: &mut Pane, axis: Axis) {
        pane.scroll_to(axis.at as f32);
        self.over = axis.at as f32 - pane.scroll_y;
        self.wrote = Some(pane.scroll_y);
    }
    /// Milliseconds on the pane's own clock, the time base the tail's velocity window reads.
    pub(super) fn ms(&mut self, now: Instant) -> f64 {
        let epoch = *self.epoch.get_or_insert(now);
        now.saturating_duration_since(epoch).as_secs_f64() * 1000.
    }
    /// A press holds the tail, the return and a notch glide where they are drawn, for the row under it.
    pub(super) fn hold(&mut self, pane: &mut Pane) {
        if self.glide.is_some() {
            pane.scroll_to(super::glided(self.glide.take(), pane.scroll_y));
            if let Some(frame) = &mut self.frame {
                frame.scroll = pane.scroll_y;
            }
            self.scrolled(pane.scroll_y, &mut pane.cursor);
        }
        if let Some(mut axis) = self.axis(pane.scroll_y) {
            self.kinetic.press(&mut axis);
            self.apply_axis(pane, axis);
        }
    }
    /// A release returns a position a press held past an end; true when that return now runs.
    pub(super) fn release(&mut self, pane: &mut Pane, now: Instant, reduced: bool) -> bool {
        let Some(mut axis) = self.axis(pane.scroll_y) else {
            return false;
        };
        if self.kinetic.running() || !self.kinetic.overscrolled(&axis) {
            return false;
        }
        self.kinetic.start_return(&mut axis, reduced);
        self.apply_axis(pane, axis);
        self.motion_at.get_or_insert(now);
        true
    }
    /// A position the motion did not write, a cursor reveal, a restore or the scrollbar, ends the motion there.
    fn settle(&mut self, pane: &Pane) {
        if self.wrote.is_some_and(|wrote| wrote != pane.scroll_y) {
            self.halt();
        }
    }
    /// One presented frame of whichever motion runs; a position the motion did not write ends it first.
    pub fn step_motion(&mut self, now: Instant, pane: &mut Pane, reduced: bool) {
        self.settle(pane);
        let dt = self.motion_at.map_or(0., |at| {
            now.saturating_duration_since(at).as_secs_f64() * 1000.
        });
        if self.kinetic.running() {
            if let Some(mut axis) = self.axis(pane.scroll_y) {
                let before = pane.scroll_y;
                self.kinetic.advance(&mut axis, dt, reduced);
                self.apply_axis(pane, axis);
                if pane.scroll_y != before {
                    self.scrolled(pane.scroll_y, &mut pane.cursor);
                    self.moved_at = Some(now);
                }
            }
        }
        if let Some(glide) = &mut self.glide {
            glide.elapsed += dt as f32;
            if f64::from(glide.elapsed) >= scroll::RETURN_MS || glide.to != pane.scroll_y {
                self.glide = None;
            }
        }
        self.motion_at = self.moving().then_some(now);
    }
}

impl Shell {
    /// A release on a drawn pane scroll (List, Dual, Columns) returns a held overscroll, a refused or menu one included, as Flickable's release fixup does.
    pub(crate) fn pointer_release(&mut self, side: usize, cx: &mut Context<Self>) {
        // Grid owns its scroll; the parked List offset does not belong to its frame.
        if self.view == View::Grid {
            return;
        }
        let (now, reduced) = (cx.background_executor().now(), cx.reduce_motion());
        if let Some(host) = &mut self.panes[side] {
            if host.input.release(&mut host.pane, now, reduced) {
                cx.notify();
            }
        }
    }
    pub(crate) fn pointer_wheel(
        &mut self,
        side: usize,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let blocked = self.wheel_blocked(cx);
        let reduced = cx.reduce_motion();
        let now = cx.background_executor().now();
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        // Membership sees every phase event, a refused one included, so a lift a busy pane drops still closes the stroke.
        let in_stroke = host.input.stroke || event.touch_phase != TouchPhase::Moved;
        host.input.stroke = match event.touch_phase {
            TouchPhase::Started => true,
            TouchPhase::Ended | TouchPhase::Cancelled => false,
            TouchPhase::Moved => host.input.stroke,
        };
        if blocked || host.pane.busy() || host.input.band.is_some() {
            return;
        }
        let input = &mut host.input;
        input.settle(&host.pane);
        let Some(mut axis) = input.axis(host.pane.scroll_y) else {
            return;
        };
        let Some(wheel) = delivery(event, in_stroke) else {
            input.kinetic.press(&mut axis);
            input.kinetic.start_return(&mut axis, true);
            input.apply_axis(&mut host.pane, axis);
            cx.notify();
            return;
        };
        let from = input.drawn(host.pane.scroll_y);
        let before = host.pane.scroll_y;
        let at = input.ms(now);
        let consumed = input.kinetic.wheel(&mut axis, wheel, at, reduced);
        input.apply_axis(&mut host.pane, axis);
        if wheel.angle != 0. && !reduced && host.pane.scroll_y != before {
            input.glide = Some(Glide {
                from,
                to: host.pane.scroll_y,
                elapsed: 0.,
            });
        }
        // The wheel lands under the pointer, so the row the scroll brings there takes the hover once the frame is drawn.
        input.pointer_origin = true;
        input.surface_pointer = Some(event.position);
        input.last_pointer = Some(event.position);
        if before != host.pane.scroll_y {
            input.scrolled(host.pane.scroll_y, &mut host.pane.cursor);
            input.moved(now);
        }
        if consumed {
            cx.stop_propagation();
        }
        input.motion_at.get_or_insert(now);
        self.arm_input_tick(side, window, cx);
        cx.notify();
    }
}
