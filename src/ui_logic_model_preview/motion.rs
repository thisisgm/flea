//! The 3D view's motion on an explicit clock: the turntable, release inertia and key or reset tweens, all Omarchy OutCubic.
use super::camera::{clamp_pitch, View};
use std::collections::VecDeque;
use std::f32::consts::PI;

/// One turn of the turntable takes this long.
pub const TURN_SECONDS: f32 = 24.;
/// The turntable eases in from rest over this long.
pub const EASE_IN: f32 = 0.6;
/// A released drag glides on and settles over this long.
pub const INERTIA: f32 = 0.8;
/// An orbit key turns this far.
pub const ORBIT_KEY: f32 = 15. * PI / 180.;
/// A zoom key steps the scale by this factor.
pub const ZOOM_KEY: f32 = 1.25;
pub const ORBIT_KEY_MS: f32 = 250.;
pub const ZOOM_KEY_MS: f32 = 200.;
pub const RESET_MS: f32 = 300.;
// A frame never advances the motion by more than this, so a stalled frame does not jump.
const MAX_STEP: f32 = 0.05;
// The drag samples the release velocity is measured over, and how fresh the last must be to glide.
const VELOCITY_WINDOW: f32 = 0.09;
const RELEASE_FRESH: f32 = 0.06;
const MIN_SPAN: f32 = 0.016;

/// Omarchy's OutCubic, 0 to 1 over 0 to 1.
pub fn out_cubic(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    1. - (1. - t).powi(3)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Inertia {
    v_az: f32,
    v_el: f32,
    t0: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Tween {
    from: View,
    to: View,
    t0: f64,
    ms: f32,
}

/// What moves the view between frames; every time is in seconds on the caller's clock.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Motion {
    spin: Option<f64>,
    inertia: Option<Inertia>,
    tween: Option<Tween>,
    last: Option<f64>,
}

impl Motion {
    /// Starts the turntable from rest at `now`, easing in.
    pub fn start_spin(&mut self, now: f64) {
        self.spin = Some(now);
        self.inertia = None;
        self.last = Some(now);
    }

    pub fn stop_spin(&mut self) {
        self.spin = None;
    }

    pub fn spinning(&self) -> bool {
        self.spin.is_some()
    }

    /// A drag, wheel or orbit key takes hold: the turn stops where it is and any glide or tween ends.
    pub fn interact(&mut self) {
        self.spin = None;
        self.inertia = None;
        self.tween = None;
    }

    /// Whether anything still moves the view.
    pub fn moving(&self) -> bool {
        self.spin.is_some() || self.inertia.is_some() || self.tween.is_some()
    }

    /// Pauses every motion's clock, as an unfocused window or hidden preview does; `resume` restarts the turn from rest.
    pub fn pause(&mut self) {
        self.last = None;
    }

    pub fn resume(&mut self, now: f64) {
        if self.spin.is_some() {
            self.spin = Some(now);
        }
        self.last = Some(now);
    }

    /// Glides from `from` to `to` over `ms` milliseconds, turning the short way round.
    pub fn tween(&mut self, from: View, mut to: View, ms: f32, now: f64) {
        let turn = to.az - from.az;
        to.az = from.az + turn.sin().atan2(turn.cos());
        self.inertia = None;
        self.tween = Some(Tween { from, to, t0: now, ms });
        self.last = Some(now);
    }

    /// A released drag carries on at this angular velocity in radians per second.
    pub fn glide(&mut self, v_az: f32, v_el: f32, now: f64) {
        self.inertia = Some(Inertia { v_az, v_el, t0: now });
        self.last = Some(now);
    }

    /// Advances the view to `now`; answers whether it moved, which is whether another frame is wanted.
    pub fn advance(&mut self, view: &mut View, now: f64) -> bool {
        let dt = self.last.map_or(0., |last| ((now - last) as f32).clamp(0., MAX_STEP));
        self.last = Some(now);
        let mut moving = false;
        if let Some(start) = self.spin {
            let ease = out_cubic((now - start) as f32 / EASE_IN);
            view.az += 2. * PI / TURN_SECONDS * ease * dt;
            moving = true;
        }
        if let Some(glide) = self.inertia {
            let t = (now - glide.t0) as f32 / INERTIA;
            if t >= 1. {
                self.inertia = None;
            } else {
                let k = (1. - t).powi(2);
                view.az += glide.v_az * k * dt;
                view.el = clamp_pitch(view.el + glide.v_el * k * dt);
                moving = true;
            }
        }
        if let Some(tw) = self.tween {
            let t = out_cubic((now - tw.t0) as f32 * 1000. / tw.ms.max(1.));
            let lerp = |a: f32, b: f32| a + (b - a) * t;
            *view = View {
                az: lerp(tw.from.az, tw.to.az),
                el: lerp(tw.from.el, tw.to.el),
                scale: lerp(tw.from.scale, tw.to.scale),
                cx: lerp(tw.from.cx, tw.to.cx),
                cy: lerp(tw.from.cy, tw.to.cy),
                target: [0, 1, 2].map(|i| lerp(tw.from.target[i], tw.to.target[i])),
            };
            if t >= 1. {
                *view = tw.to;
                self.tween = None;
            }
            moving = true;
        }
        moving
    }
}

/// The last moments of a drag, from which a release measures its glide.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DragTrack {
    samples: VecDeque<(f64, f32, f32)>,
}

impl DragTrack {
    /// Records the view's turn and pitch at `now`, keeping the last 90 ms and never fewer than two samples.
    pub fn record(&mut self, now: f64, view: &View) {
        self.samples.push_back((now, view.az, view.el));
        while self.samples.len() > 2 && (now - self.samples[0].0) as f32 > VELOCITY_WINDOW {
            self.samples.pop_front();
        }
    }

    /// The glide a release at `now` carries on with, none when the pointer had already stopped.
    pub fn release(&self, now: f64) -> Option<(f32, f32)> {
        let (first, last) = (self.samples.front()?, self.samples.back()?);
        if self.samples.len() < 2 || (now - last.0) as f32 >= RELEASE_FRESH {
            return None;
        }
        let span = ((last.0 - first.0) as f32).max(MIN_SPAN);
        Some(((last.1 - first.1) / span, (last.2 - first.2) / span))
    }
}
