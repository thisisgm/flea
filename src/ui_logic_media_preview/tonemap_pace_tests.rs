use super::tonemap_pace::{
    Look, Pace, Timespec, Verdict, CLOCK_PROCESS_CPUTIME_ID, SETTLE_MS, WINDOW_MS,
};

const RATE_30: u32 = 30_000;
const CPUS: u32 = 12;

/// How play runs while the simulation looks at it.
#[derive(Clone, Copy)]
struct Play {
    /// Percent of a 30 fps clip's frames presented.
    share: u64,
    /// Percent of the CPUs the helper keeps busy.
    busy: u64,
    /// Clip ms that pass per 10 ms of wall time.
    clock: i64,
}

const HELD: Play = Play {
    share: 100,
    busy: 70,
    clock: 10,
};

fn short(share: u64, busy: u64) -> Play {
    Play {
        share,
        busy,
        clock: 10,
    }
}

/// Plays `ms` of wall time from `look` in 10 ms looks. Returns the last look and every verdict that was not Wait.
fn play(pace: &mut Pace, mut look: Look, ms: u64, how: Play) -> (Look, Vec<Verdict>) {
    let mut verdicts = Vec::new();
    let (mut owed, mut cpu_owed) = (0_u64, 0_u64);
    for _ in 0..ms / 10 {
        look.wall_ms += 10;
        look.position_ms += how.clock;
        // 30 frames a second is 3 frames every 100 ms; `share` of them are presented.
        owed += 3 * how.share;
        look.presented += owed / 1000;
        owed %= 1000;
        // 10 ms of wall time on every CPU, `busy` percent of it used.
        cpu_owed += 10 * u64::from(CPUS) * how.busy;
        look.cpu_ms += cpu_owed / 100;
        cpu_owed %= 100;
        match pace.observe(look, RATE_30, CPUS) {
            Verdict::Wait => {}
            verdict => verdicts.push(verdict),
        }
    }
    (look, verdicts)
}

fn all_hold(verdicts: &[Verdict]) -> bool {
    verdicts.iter().all(|v| matches!(v, Verdict::Holds(_)))
}

#[test]
fn pv3_a_clip_that_presents_its_frames_holds_its_size() {
    let mut pace = Pace::default();
    let (_, verdicts) = play(&mut pace, Look::default(), 20_000, HELD);
    assert!(!verdicts.is_empty());
    assert!(
        verdicts
            .iter()
            .all(|v| matches!(v, Verdict::Holds(share) if *share >= 95)),
        "{verdicts:?}"
    );
}

#[test]
fn pv3_a_short_clip_on_busy_cpus_steps_once_then_holds_after_the_step() {
    let mut pace = Pace::default();
    let (look, verdicts) = play(
        &mut pace,
        Look::default(),
        SETTLE_MS + WINDOW_MS + 10,
        short(50, 90),
    );
    assert!(
        matches!(verdicts[..], [Verdict::Step(48..=52)]),
        "{verdicts:?}"
    );
    // The helper sets the smaller filter, which restarts the measurement.
    pace.restart();
    let (_, verdicts) = play(&mut pace, look, 10_000, HELD);
    assert!(!verdicts.is_empty() && all_hold(&verdicts), "{verdicts:?}");
}

#[test]
fn pv3_a_collapsed_clip_keeps_stepping_while_the_cpus_stay_busy() {
    let mut pace = Pace::default();
    let mut look = Look::default();
    for _ in 0..3 {
        let (next, verdicts) = play(&mut pace, look, SETTLE_MS + WINDOW_MS + 10, short(16, 90));
        assert!(
            matches!(verdicts[..], [Verdict::Step(14..=18)]),
            "{verdicts:?}"
        );
        pace.restart();
        look = next;
    }
}

#[test]
fn pv3_a_clip_short_with_idle_cpus_never_steps() {
    let mut pace = Pace::default();
    let (_, verdicts) = play(&mut pace, Look::default(), 20_000, short(40, 20));
    assert!(!verdicts.is_empty());
    assert!(
        verdicts.iter().all(|v| matches!(v, Verdict::Elsewhere(_))),
        "{verdicts:?}"
    );
}

#[test]
fn pv3_a_clip_clock_that_falls_behind_counts_as_short() {
    let mut pace = Pace::default();
    let slow = Play {
        share: 50,
        busy: 95,
        clock: 5,
    };
    let (_, verdicts) = play(&mut pace, Look::default(), SETTLE_MS + WINDOW_MS + 10, slow);
    assert!(
        matches!(verdicts[..], [Verdict::Step(48..=52)]),
        "{verdicts:?}"
    );
}

#[test]
fn pv3_frames_lost_in_the_first_second_never_count() {
    let mut pace = Pace::default();
    let (look, _) = play(&mut pace, Look::default(), SETTLE_MS - 10, short(0, 100));
    let (_, verdicts) = play(&mut pace, look, 10_000, HELD);
    assert!(!verdicts.is_empty() && all_hold(&verdicts), "{verdicts:?}");
}

#[test]
fn pv3_a_seek_starts_the_measurement_over() {
    let mut pace = Pace::default();
    let (mut look, verdicts) = play(&mut pace, Look::default(), SETTLE_MS + WINDOW_MS / 2, HELD);
    assert!(verdicts.is_empty());
    // A backwards seek the helper did not report, then the chain catching up with nothing presented for 0.9 s.
    look.position_ms = 500;
    let (look, verdicts) = play(&mut pace, look, SETTLE_MS - 100, short(0, 100));
    assert!(verdicts.is_empty(), "{verdicts:?}");
    let (mut look, verdicts) = play(&mut pace, look, 6_000, HELD);
    assert!(!verdicts.is_empty() && all_hold(&verdicts), "{verdicts:?}");
    // A forward jump past the wall clock a full window on starts over instead of judging the window.
    look.wall_ms += WINDOW_MS;
    look.position_ms += 60_000;
    assert_eq!(pace.observe(look, RATE_30, CPUS), Verdict::Wait);
    let (_, verdicts) = play(&mut pace, look, SETTLE_MS - 100, short(0, 100));
    assert!(verdicts.is_empty(), "{verdicts:?}");
}

#[test]
fn pv3_a_pause_presents_nothing_and_counts_nothing() {
    let mut pace = Pace::default();
    let (look, _) = play(&mut pace, Look::default(), SETTLE_MS + WINDOW_MS / 2, HELD);
    // Paused: the helper restarts on the pause change, then clip clock and count stand still while wall time runs.
    pace.restart();
    let paused = Play {
        share: 0,
        busy: 1,
        clock: 0,
    };
    let (look, verdicts) = play(&mut pace, look, 30_000, paused);
    assert!(verdicts.is_empty(), "{verdicts:?}");
    pace.restart();
    let (_, verdicts) = play(&mut pace, look, 6_000, HELD);
    assert!(!verdicts.is_empty() && all_hold(&verdicts), "{verdicts:?}");
}

#[test]
fn pv3_an_unknown_frame_rate_or_cpu_count_never_steps() {
    let mut pace = Pace::default();
    let mut look = Look::default();
    for _ in 0..2_000 {
        look.wall_ms += 10;
        look.position_ms += 10;
        look.cpu_ms += 120;
        assert_eq!(pace.observe(look, 0, CPUS), Verdict::Wait);
        assert_eq!(pace.observe(look, RATE_30, 0), Verdict::Wait);
    }
}

#[test]
fn pv3_the_hand_declared_cpu_clock_matches_libc() {
    assert_eq!(CLOCK_PROCESS_CPUTIME_ID, libc::CLOCK_PROCESS_CPUTIME_ID);
    assert_eq!(
        std::mem::size_of::<Timespec>(),
        std::mem::size_of::<libc::timespec>()
    );
    assert!(super::tonemap_pace::process_cpu_ms().is_some());
}
