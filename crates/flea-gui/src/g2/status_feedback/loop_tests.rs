//! Caption scheduling on GPUI's executor clock, including actual tracked-text paints.
use super::caption::PAINTED;
use super::hero_tests::{frame, open, read, run, FRAME};
use flea::ui_logic_status_feedback::empty::{PHRASES, ROTATE_MS};
use flea::ui_logic_status_feedback::{CLOSE_MS, OPEN_MS};
use gpui::TestAppContext;

#[gpui::test]
fn captions_loop_three_cycles_with_one_timer_and_no_idle_frames_or_paints(cx: &mut TestAppContext) {
    let handle = open(cx);
    run(cx, handle, 2400);
    let mut elapsed = 2400;
    for cycle in 1..=3 {
        let (mut frames, mut paints, mut idle) = (0, 0, 0);
        for phrase in 1..=PHRASES.len() {
            let timers = read(cx, handle, |hero| hero.counters.timers.get());
            for _ in 0..ROTATE_MS / FRAME {
                let before = read(cx, handle, |hero| {
                    (hero.counters.frames.get(), hero.counters.renders.get())
                });
                PAINTED.with(|painted| *painted.borrow_mut() = Some(Vec::new()));
                frame(cx, handle);
                let painted = PAINTED.with(|painted| painted.borrow_mut().take().unwrap());
                let after = read(cx, handle, |hero| {
                    (hero.counters.frames.get(), hero.counters.renders.get())
                });
                elapsed += FRAME;
                let requested = after.0 - before.0;
                let painted = usize::from(!painted.is_empty());
                frames += requested;
                paints += painted;
                if elapsed % ROTATE_MS >= CLOSE_MS + OPEN_MS + FRAME {
                    assert_eq!(
                        (requested, painted, after.1 - before.1),
                        (0, 0, 0),
                        "idle at {elapsed}ms"
                    );
                    idle += 1;
                }
            }
            let beat = (cycle - 1) * PHRASES.len() + phrase;
            assert_eq!(
                read(cx, handle, |hero| (
                    hero.rotation().beats(),
                    hero.rotation().index()
                )),
                (beat, phrase % PHRASES.len())
            );
            assert_eq!(
                read(cx, handle, |hero| hero.counters.timers.get()) - timers,
                1,
                "one timer after beat {beat}"
            );
        }
        assert!(frames > 0 && paints > 0);
        eprintln!("FB1_CAPTION_CYCLE cycle={cycle} beats={} frame_requests={frames} caption_paints={paints} idle_samples={idle} idle_frame_requests=0 idle_paints=0", PHRASES.len());
    }
}

#[gpui::test]
fn enabling_reduced_motion_cancels_the_waiting_beat(cx: &mut TestAppContext) {
    let handle = open(cx);
    run(cx, handle, 2400);
    cx.update(|cx| cx.set_reduce_motion(true));
    frame(cx, handle);
    let before = read(cx, handle, |hero| {
        (hero.counters.renders.get(), hero.counters.timers.get())
    });
    assert_eq!(run(cx, handle, 3 * PHRASES.len() as u64 * ROTATE_MS), 0);
    assert_eq!(
        read(cx, handle, |hero| (
            hero.counters.renders.get(),
            hero.counters.timers.get()
        )),
        before
    );
}

#[gpui::test]
fn a_hidden_window_cancels_caption_work_until_a_fresh_appearance(cx: &mut TestAppContext) {
    let handle = open(cx);
    run(cx, handle, ROTATE_MS + CLOSE_MS / 2);
    assert!(
        read(cx, handle, |hero| hero.rotation().caption_animating()),
        "the hide lands mid-fade"
    );
    let asked = read(cx, handle, |hero| hero.counters.frames.get());
    cx.simulate_window_visibility_change(handle.into(), gpui::WindowVisibility::Hidden);
    frame(cx, handle);
    assert_eq!(
        read(cx, handle, |hero| hero.counters.frames.get()),
        asked,
        "no animation frame is requested after the hide"
    );
    let counts = |cx: &mut TestAppContext| {
        read(cx, handle, |hero| {
            (
                hero.counters.renders.get(),
                hero.counters.frames.get(),
                hero.counters.timers.get(),
            )
        })
    };
    let hidden = counts(cx);
    assert_eq!(run(cx, handle, 3 * PHRASES.len() as u64 * ROTATE_MS), 0);
    assert_eq!(counts(cx), hidden);
    cx.simulate_window_visibility_change(handle.into(), gpui::WindowVisibility::Visible);
    frame(cx, handle);
    assert_eq!(
        read(cx, handle, |hero| (
            hero.rotation().index(),
            hero.rotation().beats()
        )),
        (0, 0)
    );
    let resumed = counts(cx);
    assert_eq!(run(cx, handle, ROTATE_MS - 2 * FRAME), 0);
    assert_eq!(counts(cx), resumed);
    run(cx, handle, 3 * FRAME);
    assert_eq!(read(cx, handle, |hero| hero.rotation().beats()), 1);
}
