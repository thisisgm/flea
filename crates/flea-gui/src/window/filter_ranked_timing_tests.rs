//! Explicit measurements of the production input paths.
use super::*;

fn rows(order: &str) -> Rc<[usize]> {
    let mut rows = (0..100_000).map(|at| at * 2).collect::<Vec<_>>();
    if order == "ranked" {
        rows.rotate_left(50_000);
    }
    rows.into()
}

#[gpui::test]
#[ignore = "FX-FILTERNAV cursorDown timing, run explicitly with --ignored --nocapture"]
fn ranked_filter_cursor_down_100k_timing(cx: &mut TestAppContext) {
    let handle = ranked(cx, View::List, &RANKED);
    edit(cx, handle, |shell, window, cx| {
        for order in ["ascending", "ranked"] {
            let host = shell.panes[0].as_mut().unwrap();
            host.input.set_shown(Some(rows(order)));
            host.pane.held = Rc::new(crate::pane::Held {
                listing: 2,
                total: 200_000,
                start: 0,
                rows: host.pane.held.rows.clone(),
            });
            host.input.frame.as_mut().unwrap().total = 100_000;
            host.input.frame.as_mut().unwrap().content = 2_000_000.;
            let from = if order == "ranked" { 50_000 } else { 199_996 };
            for batch in 0..24 {
                let mut times = Vec::new();
                for _ in 0..1000 {
                    shell.panes[0].as_mut().unwrap().pane.cursor = from;
                    let start = std::time::Instant::now();
                    assert!(shell.input_action("cursorDown", window, cx));
                    times.push(start.elapsed().as_nanos());
                    assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, from + 2);
                }
                times.sort_unstable();
                eprintln!("CURSOR_DOWN_100K order={order} batch={batch} min_ns={} median_ns={} p95_ns={} max_ns={}",
                times[0], times[500], times[950], times[999]);
            }
        }
    });
}

#[gpui::test]
#[ignore = "FX-FILTERNAV kinetic frame timing, run explicitly with --ignored --nocapture"]
fn ranked_filter_wheel_step_100k_timing(cx: &mut TestAppContext) {
    use crate::input::scroll::{Axis, Limits, Phase, Wheel};
    use std::time::{Duration, Instant};
    let handle = ranked(cx, View::List, &RANKED);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.held = Rc::new(crate::pane::Held {
            listing: 2,
            total: 200_000,
            start: 0,
            rows: host.pane.held.rows.clone(),
        });
        host.input.frame.as_mut().unwrap().total = 100_000;
        host.input.frame.as_mut().unwrap().content = 2_000_000.;
        host.pane.scroll_extent = Some(2_000_000.);
        for order in ["ascending", "ranked"] {
            host.input.set_shown(Some(rows(order)));
            for batch in 0..24 {
                let mut times = Vec::new();
                for _ in 0..1000 {
                    host.input.halt();
                    let mut axis = Axis {
                        at: 1_500_000.,
                        limits: Limits {
                            min: 0.,
                            max: 1_999_920.,
                        },
                        size: 80.,
                        interactive: true,
                    };
                    for (phase, now) in [
                        (Phase::Begin, 0.),
                        (Phase::Update, 8.),
                        (Phase::Update, 16.),
                        (Phase::End, 24.),
                    ] {
                        host.input.kinetic.wheel(
                            &mut axis,
                            Wheel {
                                phase,
                                pixel: if phase == Phase::End { 0. } else { -12. },
                                ..Default::default()
                            },
                            now,
                            false,
                        );
                    }
                    assert!(host.input.kinetic.tail_active());
                    host.pane.scroll_to(axis.at as f32);
                    host.pane.cursor = if order == "ranked" { 50_000 } else { 199_998 };
                    let now = Instant::now();
                    host.input.step_motion(now, &mut host.pane, false);
                    let before = host.pane.scroll_y;
                    let start = Instant::now();
                    host.input
                        .step_motion(now + Duration::from_millis(16), &mut host.pane, false);
                    times.push(start.elapsed().as_nanos());
                    assert!(host.pane.scroll_y > before);
                    assert!(host.pane.cursor < 199_998);
                }
                times.sort_unstable();
                eprintln!("WHEEL_STEP_100K order={order} batch={batch} min_ns={} median_ns={} p95_ns={} max_ns={}",
                times[0], times[500], times[950], times[999]);
            }
        }
    });
}
