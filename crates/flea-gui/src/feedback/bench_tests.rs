//! Headless cost of the shipped Omi arms, including upstream sampling allocations.
use super::spinner::{Arm, Crawl};
use super::spinner_tests::{frame, land, load, open_with, shell, Holds};
use crate::{
    list::bench::{allocations, real_cx},
    state::Shell,
};
use gpui::{
    div, prelude::*, App, Bounds, Context, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, Render, Style, StyleRefinement, Window,
};
use std::{rc::Rc, time::Instant};

struct Bench {
    shell: Entity<Shell>,
    arm: Arm,
}
impl Holds for Bench {
    fn held(&self) -> &Entity<Shell> {
        &self.shell
    }
}
impl Render for Bench {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .child(
                self.shell
                    .clone()
                    .cached(StyleRefinement::default().size_full()),
            )
            .child(Paint(self.shell.clone(), self.arm))
    }
}
struct Paint(Entity<Shell>, Arm);
impl IntoElement for Paint {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Paint {
    type RequestLayoutState = ();
    type PrepaintState = Rc<Crawl>;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (window.request_layout(Style::default(), None, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Rc<Crawl> {
        let mark = cx.read_entity(&self.0, |shell, _| {
            Rc::clone(&shell.panes[0].as_ref().unwrap().pane.swap.mark)
        });
        match self.1 {
            Arm::Crawl => mark.prepare(true, None, window, cx),
            Arm::Pinned(seconds) => mark.prepare(false, Some(seconds), window, cx),
            Arm::Hidden => {}
        }
        mark
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        mark: &mut Rc<Crawl>,
        window: &mut Window,
        _: &mut App,
    ) {
        if self.1 != Arm::Hidden {
            mark.paint(window);
        }
    }
}
fn cpu_ns() -> u64 {
    std::fs::read_to_string("/proc/thread-self/schedstat")
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}
fn range(mut values: Vec<f64>) -> String {
    values.sort_by(f64::total_cmp);
    format!(
        "{:.2}..{:.2} median {:.2}",
        values[0],
        values[values.len() - 1],
        values[values.len() / 2]
    )
}

#[test]
#[ignore]
fn arms() {
    for arm in [Arm::Crawl, Arm::Pinned(0.713), Arm::Hidden] {
        let mut cx = real_cx();
        let (handle, _keep) = open_with(&mut cx, true, |shell, _, _| Bench { shell, arm });
        let mark = load(&mut cx, handle);
        shell(&mut cx, handle, |shell, _, _| shell.deadlines[0] = None);
        let mut cpu = Vec::new();
        let mut allocs = Vec::new();
        let mut p99 = Vec::new();
        for batch in 0..8 {
            let (start, before) = (cpu_ns(), allocations());
            let mut times = Vec::with_capacity(1200);
            for _ in 0..1200 {
                let now = Instant::now();
                if arm != Arm::Crawl {
                    handle.update(&mut cx, |_, _, cx| cx.notify()).unwrap();
                }
                frame(&mut cx, handle);
                times.push(now.elapsed().as_secs_f64() * 1e6);
            }
            let used = cpu_ns() - start;
            let allocated = allocations() - before;
            if batch > 0 {
                cpu.push(used as f64 / 1200. / 1000.);
                allocs.push(allocated as f64 / 1200.);
                times.sort_by(f64::total_cmp);
                p99.push(times[1187]);
            }
        }
        assert_eq!(mark.counters.others.get(), 0);
        println!("SPINBENCH arm={arm:?} cpu_us_per_frame={} wall_p99_us={} allocs_per_frame={} pane_renders={}", range(cpu), range(p99), range(allocs), mark.counters.others.get());
    }
}

#[test]
#[ignore]
fn hidden() {
    let mut cx = real_cx();
    let (handle, _keep) = open_with(&mut cx, true, |shell, _, _| Bench {
        shell,
        arm: Arm::Crawl,
    });
    let mark = load(&mut cx, handle);
    for _ in 0..30 {
        frame(&mut cx, handle);
    }
    land(&mut cx, handle);
    let count = (mark.counters.frames.get(), mark.counters.requests.get());
    let now = cpu_ns();
    for _ in 0..125 {
        assert_eq!(frame(&mut cx, handle), 0);
    }
    assert_eq!(
        count,
        (mark.counters.frames.get(), mark.counters.requests.get())
    );
    println!(
        "SPINBENCH hidden cpu_us_per_2s={:.2} frames=0 requests=0",
        (cpu_ns() - now) as f64 / 1000.
    );
}

#[test]
fn still_targets_settle_and_resume_without_reviving_morphs() {
    use super::spinner::{Mode, Motion};
    let now = Instant::now();
    for side in [22., 44., 88.] {
        let mut motion = Motion::new(Mode::Thinking);
        motion.sample(true, None, side, now);
        motion.set(Mode::Working);
        motion.sample(
            true,
            None,
            side,
            now + std::time::Duration::from_millis(160),
        );
        assert!(!motion.settled());
        motion.sample(false, None, side, now + std::time::Duration::from_secs(1));
        assert!(motion.settled());
        for (index, mode) in [Mode::Working, Mode::Upload, Mode::Thinking, Mode::Download]
            .into_iter()
            .enumerate()
        {
            motion.set(mode);
            assert!(motion.settled());
            let rest = omi::Player::with_options(
                &omi::Kit::bundled(),
                omi::Options {
                    body_motion: side >= 44.,
                    ..Default::default()
                },
            )
            .mode_rects(mode.id(), 0.)
            .unwrap();
            let time = now + std::time::Duration::from_secs(10 + index as u64 * 10);
            for sample in 0..8 {
                motion.sample(
                    false,
                    None,
                    side,
                    time + std::time::Duration::from_millis(sample * 100),
                );
                assert_eq!(motion.rects(), rest);
                assert!(motion.settled());
            }
            let resumed = time + std::time::Duration::from_secs(2);
            motion.sample(true, None, side, resumed);
            assert_eq!(motion.rects(), rest);
            assert!(motion.settled());
            let mut loop_moved = false;
            for tick in 1..=200 {
                motion.sample(
                    true,
                    None,
                    side,
                    resumed + std::time::Duration::from_millis(tick * 16),
                );
                assert!(motion.settled());
                assert!(!motion.rects().is_empty());
                loop_moved |= motion.rects() != rest;
            }
            assert!(loop_moved, "{mode:?} at {side} must resume its loop");
            motion.sample(false, None, side, time + std::time::Duration::from_secs(6));
        }
    }
}

#[test]
fn pinned_samples_keep_their_time_body_motion_and_target_on_resume() {
    use super::spinner::{Mode, Motion};
    let now = Instant::now();
    for side in [22., 44., 88.] {
        let mut motion = Motion::new(Mode::Thinking);
        motion.sample(true, None, side, now);
        motion.set(Mode::Working);
        motion.sample(
            true,
            None,
            side,
            now + std::time::Duration::from_millis(160),
        );
        for mode in [Mode::Working, Mode::Upload, Mode::Download, Mode::Thinking] {
            motion.set(mode);
            let reference = omi::Player::with_options(
                &omi::Kit::bundled(),
                omi::Options {
                    body_motion: side >= 44.,
                    ..Default::default()
                },
            );
            for seconds in [0., 0.713, 2.75] {
                motion.sample(
                    false,
                    Some(seconds),
                    side,
                    now + std::time::Duration::from_secs(5),
                );
                assert_eq!(motion.seconds(), seconds);
                assert_eq!(
                    motion.rects(),
                    reference.mode_rects(mode.id(), f64::from(seconds)).unwrap()
                );
                assert!(motion.settled());
            }
            motion.sample(true, None, side, now + std::time::Duration::from_secs(6));
            assert_eq!(motion.rects(), reference.mode_rects(mode.id(), 0.).unwrap());
            assert!(motion.settled());
        }
    }
}
