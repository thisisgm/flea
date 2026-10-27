//! ViewportScrollBar.qml, ScrollOff.js and Scroll.js arithmetic, shared by paint, input and IPC.
use super::Frame;
#[path = "kinetic.rs"]
mod kinetic;
pub use kinetic::*;

#[derive(Clone, Copy)]
pub struct Bar {
    pub handle: f32,
    pub offset: f32,
    pub travel: f32,
}
impl Bar {
    pub fn new(frame: Frame) -> Self {
        let track = frame.viewport();
        let handle = if frame.content <= track {
            track
        } else {
            (track * track / frame.content).max(24.).min(track)
        };
        let travel = (track - handle).max(0.);
        let offset = if frame.max_scroll() > 0. {
            travel * frame.scroll.clamp(0., frame.max_scroll()) / frame.max_scroll()
        } else {
            0.
        };
        Self {
            handle,
            offset,
            travel,
        }
    }
    pub fn position(self, frame: Frame, offset: f32) -> f32 {
        if self.travel > 0. {
            offset.clamp(0., self.travel) * frame.max_scroll() / self.travel
        } else {
            0.
        }
    }
}

pub fn reveal(frame: Frame, cursor: usize, context: usize) -> f32 {
    let cursor = cursor / frame.columns.max(1);
    let total = frame.total.div_ceil(frame.columns.max(1));
    let top = cursor as f32 * frame.row;
    if context == 0 {
        return if top < frame.scroll {
            top
        } else if top + frame.row > frame.scroll + frame.viewport() {
            top + frame.row - frame.viewport()
        } else {
            frame.scroll
        }
        .clamp(0., frame.max_scroll());
    }
    let visible = (frame.viewport() / frame.row).floor().max(1.) as usize;
    let margin = context.min(visible.saturating_sub(1) / 2);
    let first = frame.grow.position_at(frame.scroll, frame.row);
    let want = if cursor < first + margin {
        cursor.saturating_sub(margin)
    } else if cursor + margin >= first + visible {
        (cursor + margin + 1).saturating_sub(visible)
    } else {
        first
    };
    let want = want.min(total.saturating_sub(visible));
    if total > visible && want == total - visible {
        return frame
            .scroll
            .max((total as f32 * frame.row - frame.viewport()).max(0.))
            .min(frame.max_scroll());
    }
    if want == first && top >= frame.scroll && top + frame.row <= frame.scroll + frame.viewport() {
        return frame.scroll;
    }
    (want as f32 * frame.row).clamp(0., frame.max_scroll())
}

/// One discrete notch is 120 units of angle delta, the platform's own convention for a wheel click.
pub const NOTCH_UNITS: f64 = 120.;
/// Lines per notch when the platform reports none.
pub const DEFAULT_LINES: f64 = 3.;
/// Pixels one line is worth and the notch multiplier, the theme's scroll pair.
pub const NOTCH_PX: f64 = super::NOTCH_PX as f64;
pub const MULTIPLIER: f64 = super::WHEEL_MULTIPLIER as f64;
/// Touchpad pixels gain 2.5; a wheel notch is unchanged.
pub const TOUCH_GAIN: f64 = super::TOUCH_GAIN as f64;
/// GPUI triples each continuous axis value (gpui-omarchy crates/gpui-pre-linux/src/linux/wayland/scroll.rs CONTINUOUS_SCALE); Qt's pixelDelta is raw.
pub const GPUI_TOUCHPAD_SCALE: f64 = 3.;
/// The fork's lines per wheel notch, which Scroll.js reads as 120 angle units.
pub const FORK_NOTCH_LINES: f64 = 3.;
/// Velocity decays by 0.998 per millisecond, so a tail is about half a second of lift velocity.
pub const MOMENTUM_DECAY: f64 = 0.998;
pub const VELOCITY_WINDOW_MS: f64 = 100.;
pub const MIN_SPAN_MS: f64 = 16.;
pub const SPAN_ANCHOR_MS: f64 = 50.;
pub const MAX_V_PX_PER_MS: f64 = 6.;
pub const STOP_V_PX_PER_MS: f64 = 0.03;
/// The rubber band's resistance, so shown overscroll stays under the viewport length.
pub const RESIST_K: f64 = 0.55;
/// Past the bound the tail brakes hard, so the overshoot peaks in a few frames.
pub const OVER_DECAY: f64 = 0.98;
/// Qt's angle delta for a scroll frame without a discrete value: 12 units per raw axis unit (qtwayland FrameData::angleDelta).
pub const CONTINUOUS_ANGLE: f64 = 12.;

/// The platform's scroll phase: a wheel carries none, any other phase is a touchpad stroke.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    None,
    Begin,
    Update,
    End,
}

pub fn is_touchpad(phase: Phase) -> bool {
    phase != Phase::None
}

/// Pixels count only for a touchpad stroke or an event carrying no angle.
pub fn moves_by_pixels(phase: Phase, pixel: f64, angle: f64) -> bool {
    pixel != 0. && (is_touchpad(phase) || angle == 0.)
}

/// Notch pixels are the platform's lines times the notch pixels times the multiplier.
pub fn distance(pixel: f64, angle: f64, lines: f64, notch_px: f64, multiplier: f64) -> f64 {
    if moves_by_pixels(Phase::None, pixel, angle) {
        return pixel;
    }
    let per_notch = if lines > 0. { lines } else { DEFAULT_LINES };
    angle / NOTCH_UNITS * per_notch * notch_px * multiplier
}

/// A GPUI touchpad pixel delta in the raw axis units every function below reads, as v0.3.12's pixelDelta.
pub fn touchpad_raw(gpui_px: f64) -> f64 {
    gpui_px / GPUI_TOUCHPAD_SCALE
}

/// A continuous frame outside a stroke as Qt's phaseless wheel event, its angle rounded to whole units; zero sends nothing.
pub fn continuous_angle(raw: f64) -> f64 {
    (raw * CONTINUOUS_ANGLE).round()
}

/// A wheel event as content pixels in GPUI's sign: a notch is the list's distance, a touchpad delta passes through.
pub fn wheel_delta(delta: &gpui::ScrollDelta) -> gpui::Point<gpui::Pixels> {
    match delta {
        gpui::ScrollDelta::Lines(lines) => {
            let notch = |lines: f32| {
                let angle = f64::from(lines) / FORK_NOTCH_LINES * NOTCH_UNITS;
                gpui::px(distance(0., angle, 0., NOTCH_PX, MULTIPLIER) as f32)
            };
            gpui::point(notch(lines.x), notch(lines.y))
        }
        gpui::ScrollDelta::Pixels(pixels) => *pixels,
    }
}

pub fn touch_distance(pixel: f64) -> f64 {
    if pixel == 0. {
        0.
    } else {
        pixel * TOUCH_GAIN
    }
}

/// One stroke delta by arrival time: across and down, both gained pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    pub t: f64,
    pub x: f64,
    pub y: f64,
}

/// Velocity over the stroke's last 100 ms; the span starts at the event before the first in-window sample.
pub fn lift_velocity(samples: &[Sample], end: f64) -> (f64, f64) {
    let (mut xs, mut ys, mut anchor, mut first, mut last, mut n) = (0., 0., -1., -1., -1., 0);
    for sample in samples {
        if sample.t <= end - VELOCITY_WINDOW_MS {
            anchor = sample.t;
        } else if sample.t <= end {
            xs += sample.x;
            ys += sample.y;
            if first < 0. {
                first = sample.t;
            }
            last = sample.t;
            n += 1;
        }
    }
    if n == 0 {
        return (0., 0.);
    }
    if anchor < 0. {
        anchor = first;
    }
    let span = MIN_SPAN_MS.max(last - anchor);
    let (mut vx, mut vy) = (xs / span, ys / span);
    let mut speed = (vx * vx + vy * vy).sqrt();
    if speed > MAX_V_PX_PER_MS {
        vx *= MAX_V_PX_PER_MS / speed;
        vy *= MAX_V_PX_PER_MS / speed;
        speed = MAX_V_PX_PER_MS;
    }
    if speed <= STOP_V_PX_PER_MS {
        return (0., 0.);
    }
    (vx, vy)
}

/// Pixels travelled in `ms` at `v0`, the closed form of the decay.
pub fn tail_travel(v0: f64, ms: f64) -> f64 {
    let t = ms.max(0.);
    if v0 == 0. || t == 0. {
        return 0.;
    }
    v0 * (MOMENTUM_DECAY.powf(t) - 1.) / MOMENTUM_DECAY.ln()
}

/// The whole tail of `v0` until it falls below the stop speed, as a positive distance.
pub fn tail_total(v0: f64) -> f64 {
    let v = v0.abs();
    if v <= STOP_V_PX_PER_MS {
        return 0.;
    }
    (v - STOP_V_PX_PER_MS) / -MOMENTUM_DECAY.ln()
}

/// One frame's exact integral, so summed frames equal the closed form: (distance, velocity after).
pub fn tail_step(v: f64, dt_ms: f64) -> (f64, f64) {
    let dt = dt_ms.max(0.);
    if v == 0. || dt == 0. {
        return (0., 0.);
    }
    let grown = MOMENTUM_DECAY.powf(dt);
    (v * (grown - 1.) / MOMENTUM_DECAY.ln(), v * grown)
}

pub fn tail_speed(vx: f64, vy: f64) -> f64 {
    (vx * vx + vy * vy).sqrt()
}

pub fn tail_live(vx: f64, vy: f64) -> bool {
    tail_speed(vx, vy) > STOP_V_PX_PER_MS
}

/// An axis's scroll bounds; short content pins to the minimum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub min: f64,
    pub max: f64,
}

/// Shown overscroll for raw travel past the bound over a viewport: 0 at 0, monotone, under the viewport.
pub fn over_resist(raw: f64, length: f64) -> f64 {
    let (raw, length) = (raw.max(0.), length.max(0.));
    if raw == 0. || length == 0. {
        return 0.;
    }
    (1. - 1. / (raw * RESIST_K / length + 1.)) * length
}

pub fn over_decel(v: f64, dt_ms: f64) -> f64 {
    v * OVER_DECAY.powf(dt_ms.max(0.))
}

/// The OutCubic curve every transition uses: fast out of the start, settling into the end.
pub fn out_cubic(t: f64) -> f64 {
    1. - (1. - t.clamp(0., 1.)).powi(3)
}

/// One OutCubic return step; at or past `duration` it lands on `to`.
pub fn return_at(from: f64, to: f64, elapsed: f64, duration: f64) -> f64 {
    let t = (elapsed / duration.max(1.)).clamp(0., 1.);
    from + (to - from) * out_cubic(t)
}

/// An event is consumed only when it moved the content, so one at an edge keeps propagating.
pub fn moved(previous: f64, current: f64) -> bool {
    (current - previous).abs() > 0.01
}
