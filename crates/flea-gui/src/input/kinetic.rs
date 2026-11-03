//! The touchpad tail, rubber-band edges and OutCubic return for one scrolled axis, advanced once per frame.
use super::*;

/// The reveal duration every return takes, the shell's OutCubic open.
pub const RETURN_MS: f64 = 180.;
// Samples a stroke keeps before it reallocates; the velocity window holds far fewer at any refresh rate.
const SAMPLE_ROOM: usize = 256;

/// The axis a handler writes: its position, bounds, viewport length and whether it takes input at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub at: f64,
    pub limits: Limits,
    pub size: f64,
    pub interactive: bool,
}

impl Axis {
    fn ranges(&self) -> bool {
        self.limits.max > self.limits.min
    }
    fn clamp(&self, value: f64) -> f64 {
        self.limits.min.max(self.limits.max.min(value))
    }
    fn outside(&self) -> bool {
        self.at < self.limits.min - 0.01 || self.at > self.limits.max + 0.01
    }
}

/// One wheel or touchpad event: phase, the pixel and angle deltas down the axis, and the platform's lines.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wheel {
    pub phase: Phase,
    pub pixel: f64,
    pub angle: f64,
    pub lines: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Glide {
    from: f64,
    to: f64,
    elapsed: f64,
}

/// One tail per scrolled view; the pane input ends it on any position it did not write.
#[derive(Debug)]
pub struct Kinetic {
    samples: Vec<Sample>,
    velocity: f64,
    active: bool,
    over: f64,
    glide: Option<Glide>,
}

impl Default for Kinetic {
    fn default() -> Self {
        Self {
            samples: Vec::with_capacity(SAMPLE_ROOM),
            velocity: 0.,
            active: false,
            over: 0.,
            glide: None,
        }
    }
}

impl Kinetic {
    pub fn tail_active(&self) -> bool {
        self.active
    }
    pub fn return_active(&self) -> bool {
        self.glide.is_some()
    }
    pub fn running(&self) -> bool {
        self.active || self.glide.is_some()
    }
    pub fn velocity(&self) -> f64 {
        self.velocity
    }
    pub fn banked(&self) -> f64 {
        self.over
    }

    pub fn stop_tail(&mut self) {
        self.active = false;
        self.velocity = 0.;
        self.samples.clear();
    }

    /// A new listing ends every motion without a write; its own landing places the view.
    pub fn halt(&mut self) {
        self.stop_tail();
        self.glide = None;
        self.over = 0.;
    }

    /// Ends the return; a snap pulls a position left past its bound back onto it.
    pub fn stop_return(&mut self, axis: &mut Axis, snap: bool) {
        self.glide = None;
        self.over = 0.;
        if snap {
            let to = axis.clamp(axis.at);
            if to != axis.at {
                self.write_raw(axis, to);
            }
        }
    }

    /// A press holds the tail and the return where they are, for the row under it.
    pub fn press(&mut self, axis: &mut Axis) {
        self.stop_tail();
        self.stop_return(axis, false);
    }

    pub fn overscrolled(&self, axis: &Axis) -> bool {
        self.over != 0. || axis.outside()
    }

    fn write(&mut self, axis: &mut Axis, value: f64) {
        let at = axis.clamp(value);
        self.write_raw(axis, at);
    }

    fn write_raw(&mut self, axis: &mut Axis, at: f64) {
        axis.at = at;
    }

    fn push_sample(&mut self, t: f64, dy: f64) {
        self.samples.push(Sample { t, x: 0., y: dy });
        let stale = self
            .samples
            .iter()
            .take_while(|sample| sample.t <= t - VELOCITY_WINDOW_MS - SPAN_ANCHOR_MS)
            .count();
        self.samples.drain(..stale);
    }

    // Banks raw travel past the bound and returns the rubber-band spot; the first overshoot banks its raw travel.
    fn bank_over(&mut self, axis: &Axis, want: f64, length: f64) -> f64 {
        let below = want < axis.limits.min;
        let beyond = if below {
            axis.limits.min - want
        } else {
            want - axis.limits.max
        };
        let previous = self.over.abs();
        let banked = if previous == 0. {
            beyond
        } else {
            previous + (beyond - over_resist(previous, length)).max(0.)
        };
        self.over = if below { -banked } else { banked };
        let shown = over_resist(banked, length);
        if below {
            axis.limits.min - shown
        } else {
            axis.limits.max + shown
        }
    }

    // Past the bound travel resists; banked travel from a swapped listing is dropped.
    fn touch(&mut self, axis: &mut Axis, delta: f64) {
        if self.over != 0. && !axis.outside() {
            self.over = 0.;
        }
        let length = axis.size.max(1.);
        if self.over == 0. {
            let want = axis.at - delta;
            if want >= axis.limits.min && want <= axis.limits.max {
                self.write(axis, want);
            } else {
                let at = self.bank_over(axis, want, length);
                self.write_raw(axis, at);
            }
            return;
        }
        let below = self.over < 0.;
        let banked = if below {
            self.over.abs() + delta
        } else {
            self.over.abs() - delta
        };
        if banked <= 0. {
            let inside = if below {
                axis.limits.min - banked
            } else {
                axis.limits.max + banked
            };
            self.over = 0.;
            self.write(axis, inside);
            return;
        }
        let shown = over_resist(banked, length);
        self.over = if below { -banked } else { banked };
        let at = if below {
            axis.limits.min - shown
        } else {
            axis.limits.max + shown
        };
        self.write_raw(axis, at);
    }

    /// A past-bound lift returns on the OutCubic open duration, at once under reduced motion.
    pub fn start_return(&mut self, axis: &mut Axis, reduced_motion: bool) {
        self.over = 0.;
        if !axis.outside() {
            self.glide = None;
            return;
        }
        let to = axis.clamp(axis.at);
        if reduced_motion {
            self.glide = None;
            self.write_raw(axis, to);
            return;
        }
        self.glide = Some(Glide {
            from: axis.at,
            to,
            elapsed: 0.,
        });
    }

    fn start_tail(&mut self, now: f64) {
        if self.over != 0. || (self.samples.is_empty() && self.active) {
            return;
        }
        let (_, velocity) = lift_velocity(&self.samples, now);
        self.samples.clear();
        self.velocity = velocity;
        self.active = velocity != 0.;
    }

    /// Routes one event; true when it was consumed, so an edge event keeps propagating.
    pub fn wheel(&mut self, axis: &mut Axis, event: Wheel, now: f64, reduced_motion: bool) -> bool {
        if !is_touchpad(event.phase) {
            if self.active {
                self.stop_tail();
            }
            if self.glide.is_some() {
                self.stop_return(axis, true);
            }
            let travel = distance(event.pixel, event.angle, event.lines, NOTCH_PX, MULTIPLIER);
            if travel == 0. || !axis.interactive {
                return false;
            }
            let previous = axis.at;
            self.write(axis, previous - travel);
            return moved(previous, axis.at);
        }
        if event.phase == Phase::Begin {
            self.stop_tail();
            self.stop_return(axis, true);
            self.push_sample(now, 0.);
        } else if event.phase != Phase::End && self.active {
            self.stop_tail();
        } else if self.glide.is_some() {
            self.stop_return(axis, true);
        }
        let down = if axis.ranges() {
            touch_distance(event.pixel)
        } else {
            0.
        };
        if down == 0. || !axis.interactive {
            if event.phase == Phase::End && axis.interactive {
                if self.overscrolled(axis) {
                    self.start_return(axis, reduced_motion);
                    return true;
                }
                self.start_tail(now);
                if self.active {
                    return true;
                }
            }
            return false;
        }
        self.push_sample(now, down);
        let previous = axis.at;
        self.touch(axis, down);
        let mut consumed = moved(previous, axis.at);
        if event.phase == Phase::End {
            if self.overscrolled(axis) {
                self.start_return(axis, reduced_motion);
            } else {
                self.start_tail(now);
            }
            consumed |= self.active;
        }
        consumed
    }

    /// One frame of whichever motion runs, the tail first; returns the distance moved.
    pub fn advance(&mut self, axis: &mut Axis, dt_ms: f64, reduced_motion: bool) -> f64 {
        if self.active {
            return self.advance_tail(axis, dt_ms, reduced_motion);
        }
        self.advance_return(axis, dt_ms)
    }

    fn advance_return(&mut self, axis: &mut Axis, dt_ms: f64) -> f64 {
        let Some(glide) = &mut self.glide else {
            return 0.;
        };
        if dt_ms <= 0. {
            return 0.;
        }
        glide.elapsed += dt_ms;
        let (next, done) = (
            return_at(glide.from, glide.to, glide.elapsed, RETURN_MS),
            glide.elapsed >= RETURN_MS,
        );
        let moved = (next - axis.at).abs();
        self.write_raw(axis, next);
        if done {
            self.glide = None;
            self.over = 0.;
        }
        moved
    }

    fn advance_tail(&mut self, axis: &mut Axis, dt_ms: f64, reduced_motion: bool) -> f64 {
        if dt_ms <= 0. {
            return 0.;
        }
        let (step, velocity) = tail_step(self.velocity, dt_ms);
        self.velocity = velocity;
        let mut moved = 0.;
        if step != 0. {
            let before = axis.at;
            let want = before - step;
            if want >= axis.limits.min && want <= axis.limits.max {
                self.write(axis, want);
                if axis.at == before {
                    self.velocity = 0.;
                }
            } else {
                let at = self.bank_over(axis, want, axis.size.max(1.));
                self.velocity = over_decel(self.velocity, dt_ms);
                self.write_raw(axis, at);
            }
            moved = (axis.at - before).abs();
        }
        if !tail_live(0., self.velocity) {
            self.stop_tail();
            if self.overscrolled(axis) {
                self.start_return(axis, reduced_motion);
            }
        }
        moved
    }
}
