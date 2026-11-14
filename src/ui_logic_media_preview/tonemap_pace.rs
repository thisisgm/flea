//! The tone map's size follows the machine: frames presented under the map count against the clip's own, and a shortfall with the CPUs busy steps it down.

/// Time after a load, seek, pause change, resize or filter change before frames count, so the chain's catch-up frames never do.
pub const SETTLE_MS: u64 = 1000;
/// Time one measurement covers: 60 frames at 30 fps, so one late frame moves the share by under 2 points.
pub const WINDOW_MS: u64 = 2000;
/// Presented share of the clip's frames, in percent, below which a window is short (minipc: held 98 to 100, overloaded 15 to 45).
pub const SHORT_PERCENT: u64 = 90;
/// Share of the helper's CPUs, in percent, a short window must keep busy to blame the map (minipc overloaded: 88 to 91; a slow disk leaves them idle).
pub const BUSY_PERCENT: u64 = 60;
/// How far the clip clock may run ahead of the wall clock in one stretch before it counts as a seek.
pub const JUMP_MS: i64 = 1000;

/// One look at play.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Look {
    /// A monotonic clock, in ms.
    pub wall_ms: u64,
    /// The clip position, in ms.
    pub position_ms: i64,
    /// Frames presented since the helper started.
    pub presented: u64,
    /// CPU time the helper has used, every thread, in ms.
    pub cpu_ms: u64,
}

/// What one look at play says about the mapped size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Still settling, the window is not full, or the clip clock stood still.
    Wait,
    /// The window presented this share of the clip's frames, enough to keep the size.
    Holds(u64),
    /// The window fell short at this share with the CPUs busy: step the size down.
    Step(u64),
    /// The window fell short at this share with the CPUs mostly idle, so the map is not the cause: the size stays.
    Elsewhere(u64),
}

/// The measurement one helper run keeps for its clip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pace {
    /// The look the current stretch began at.
    anchor: Option<Look>,
    settled: bool,
}

impl Pace {
    /// Something moved the clock or rebuilt the chain: measuring starts over, after a new settle.
    pub fn restart(&mut self) {
        self.anchor = None;
        self.settled = false;
    }

    /// One look, with the clip's rate in thousandths of a frame a second and the helper's CPUs; an unknown rate or CPU count (0) never steps.
    pub fn observe(&mut self, look: Look, rate_milli: u32, cpus: u32) -> Verdict {
        if rate_milli == 0 || cpus == 0 {
            return Verdict::Wait;
        }
        let Some(from) = self.anchor else {
            self.anchor = Some(look);
            return Verdict::Wait;
        };
        let wall = look.wall_ms.saturating_sub(from.wall_ms);
        let clip = look.position_ms - from.position_ms;
        // A seek the helper missed, a loop that was held up, or counters that went back: the stretch starts over.
        let jumped = clip < 0 || clip > wall as i64 + JUMP_MS || wall > SETTLE_MS + 2 * WINDOW_MS;
        if jumped || look.presented < from.presented || look.cpu_ms < from.cpu_ms {
            self.restart();
            self.anchor = Some(look);
            return Verdict::Wait;
        }
        if !self.settled {
            if wall >= SETTLE_MS {
                self.anchor = Some(look);
                self.settled = true;
            }
            return Verdict::Wait;
        }
        if wall < WINDOW_MS {
            return Verdict::Wait;
        }
        self.anchor = Some(look);
        // Paused, or play stopped at the end: nothing was owed.
        if clip == 0 {
            return Verdict::Wait;
        }
        let nominal = (wall * u64::from(rate_milli) / 1_000_000).max(1);
        let share = (look.presented - from.presented) * 100 / nominal;
        if share >= SHORT_PERCENT {
            return Verdict::Holds(share);
        }
        let busy = (look.cpu_ms - from.cpu_ms) * 100 / (wall * u64::from(cpus));
        if busy >= BUSY_PERCENT {
            Verdict::Step(share)
        } else {
            Verdict::Elsewhere(share)
        }
    }
}

/// clock_gettime(2)'s clock for the CPU time this process has used, every thread.
pub(super) const CLOCK_PROCESS_CPUTIME_ID: i32 = 2;

/// The 64-bit Linux timespec, seconds then nanoseconds, spelt as the lib's other clock_gettime (raster/perf_tests.rs) spells it, since a second shape clashes.
pub(super) type Timespec = [i64; 2];

extern "C" {
    fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
}

/// CPU time this process has used, every thread, in ms; none if the clock is refused.
pub fn process_cpu_ms() -> Option<u64> {
    let mut now: Timespec = [0; 2];
    // SAFETY: a valid clock id and a timespec that outlives the call.
    if unsafe { clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &mut now) } != 0 {
        return None;
    }
    let [seconds, nanoseconds] = now;
    Some(seconds.max(0) as u64 * 1000 + nanoseconds.max(0) as u64 / 1_000_000)
}
