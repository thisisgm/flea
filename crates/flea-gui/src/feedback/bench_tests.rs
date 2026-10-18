//! Measurement arms for the loading mark, run by hand: cargo test --release -p flea-gui --lib feedback::bench_tests -- --ignored --nocapture.
use super::spinner::{phase, Crawl, AT, DASH, GRID, HALF, LENGTH, PERIOD_MS, SPIRAL};
use super::spinner_tests::{frame, land, load, open_with, shell, Holds, Requests, FRAME};
use crate::{
    list::bench::{allocations, fixture, real_cx},
    state::Shell,
};
use flea::protocol::Reply;
use gpui::{
    div, point, prelude::*, px, AnyElement, App, Bounds, Context, Element, ElementId, Entity,
    GlobalElementId, InspectorElementId, LayoutId, Path, PathBuilder, Pixels, Render, Style,
    StyleRefinement, Task, TestAppContext, Window, WindowHandle,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Instant,
};

const FRAMES: usize = 1200;
const BATCHES: usize = 7;
// Phases a period cache keeps, one per frame of the walk.
const CACHED: usize = (PERIOD_MS as u128 / FRAME.as_millis()) as usize;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Geometry {
    PathEveryFrame,
    PathCache,
    Quads,
}

/// A window root like the shipped one, with the geometry, the shell's caching and a hold timer as arms.
struct Bench {
    shell: Entity<Shell>,
    geometry: Geometry,
    cached: bool,
    paths: Rc<RefCell<Vec<Path<Pixels>>>>,
    draws: Rc<Cell<usize>>,
    _timer: Option<Task<()>>,
}

impl Render for Bench {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let shell: AnyElement = if self.cached {
            self.shell
                .clone()
                .cached(StyleRefinement::default().size_full())
                .into_any_element()
        } else {
            self.shell.clone().into_any_element()
        };
        div().size_full().child(shell).child(Arm(
            self.shell.clone(),
            self.geometry,
            Rc::clone(&self.paths),
        ))
    }
}

impl Holds for Bench {
    fn held(&self) -> &Entity<Shell> {
        &self.shell
    }
}

struct Arm(Entity<Shell>, Geometry, Rc<RefCell<Vec<Path<Pixels>>>>);

impl IntoElement for Arm {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Arm {
    type RequestLayoutState = ();
    type PrepaintState = ();
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
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let mark = cx.read_entity(&self.0, |shell, _| {
            Rc::clone(&shell.panes[0].as_ref().unwrap().pane.swap.mark)
        });
        if !mark.shown() {
            return;
        }
        match self.1 {
            Geometry::Quads => mark.paint(true, None, window, cx),
            geometry => {
                let at = phase(FRAME * mark.counters.frames.get() as u32);
                mark.counters.frames.set(mark.counters.frames.get() + 1);
                let path = if geometry == Geometry::PathEveryFrame {
                    spiral(&mark, at)
                } else {
                    let mut paths = self.2.borrow_mut();
                    if paths.is_empty() {
                        paths.extend(
                            (0..CACHED).map(|step| spiral(&mark, step as f32 / CACHED as f32)),
                        );
                    }
                    paths[(at * CACHED as f32) as usize % CACHED].clone()
                };
                window.paint_path(path, mark.ink());
                window.request_animation_frame();
            }
        }
    }
}

/// The dash as a stroked path, square caps drawn as half a stroke more at each end of a run.
fn spiral(mark: &Crawl, at: f32) -> Path<Pixels> {
    let slot = mark.slot();
    let unit = f32::from(slot.size.width) / GRID;
    let place =
        |(x, y): (f32, f32)| point(slot.origin.x + px(x * unit), slot.origin.y + px(y * unit));
    let tail = AT.len() - 2;
    let along = |s: f32| {
        let at = (0..=tail).find(|&at| s <= AT[at + 1]).unwrap_or(tail);
        let ((ax, ay), (bx, by)) = (SPIRAL[at], SPIRAL[at + 1]);
        let t = (s - AT[at]) / (AT[at + 1] - AT[at]);
        (
            at,
            (ax + (bx - ax) * t, ay + (by - ay) * t),
            (
                (bx - ax) / (AT[at + 1] - AT[at]),
                (by - ay) / (AT[at + 1] - AT[at]),
            ),
        )
    };
    let mut builder = PathBuilder::stroke(px(2. * HALF * unit));
    let start = LENGTH * at;
    let mut runs = vec![(start, (start + DASH).min(LENGTH))];
    if start + DASH > LENGTH {
        runs.push((0., start + DASH - LENGTH));
    }
    for (from, to) in runs {
        let (first, (x, y), (dx, dy)) = along(from + 1e-4);
        builder.move_to(place((x - dx * (HALF + 1e-4), y - dy * (HALF + 1e-4))));
        let (last, (x, y), (dx, dy)) = along(to);
        for vertex in SPIRAL.iter().take(last + 1).skip(first + 1) {
            builder.line_to(place(*vertex));
        }
        builder.line_to(place((x + dx * HALF, y + dy * HALF)));
    }
    builder.build().unwrap()
}

// Reads the one line of /proc/thread-self/schedstat, such as "52341982 1204511 87", which is ns on a CPU, ns waiting and timeslices.
fn cpu_ns() -> u64 {
    let stat = std::fs::read_to_string("/proc/thread-self/schedstat").unwrap();
    stat.split_whitespace().next().unwrap().parse().unwrap()
}

fn open(
    cx: &mut TestAppContext,
    dual: bool,
    geometry: Geometry,
    cached: bool,
    timer: bool,
) -> (WindowHandle<Bench>, Requests) {
    let (handle, keep) = open_with(cx, dual, |shell, window, cx| {
        // A QML-style hold timer: it ticks every frame interval whether or not the mark is on screen.
        let timer = timer.then(|| {
            cx.spawn_in(window, async move |this, cx| loop {
                cx.background_executor().timer(FRAME).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            })
        });
        Bench {
            shell,
            geometry,
            cached,
            paths: Rc::default(),
            draws: Rc::default(),
            _timer: timer,
        }
    });
    if dual {
        // The second pane holds real names, so its list shapes text as a folder does.
        shell(cx, handle, |shell, _, cx| {
            let rows = Reply::Rows {
                start: 0,
                rows: fixture("2k")[..320].to_vec(),
                listing: 2,
            };
            shell.panes[1].as_mut().unwrap().pane.receive(rows).unwrap();
            cx.notify();
        });
    }
    (handle, keep)
}

/// The shared fall back to the loading mark, with the listing's 10 s wait dropped since it never answers here.
fn loading(cx: &mut TestAppContext, handle: WindowHandle<Bench>) -> Rc<Crawl> {
    let mark = load(cx, handle);
    shell(cx, handle, |shell, _, _| shell.deadlines[0] = None);
    mark
}

fn range(mut values: Vec<f64>) -> String {
    values.sort_by(f64::total_cmp);
    format!(
        "{:.1}..{:.1} median {:.1}",
        values[0],
        values[values.len() - 1],
        values[values.len() / 2]
    )
}

fn measure(geometry: Geometry, cached: bool, dual: bool) {
    let mut cx = real_cx();
    let (handle, _keep) = open(&mut cx, dual, geometry, cached, false);
    let mark = loading(&mut cx, handle);
    for _ in 0..FRAMES {
        frame(&mut cx, handle);
    }
    let draws = handle
        .update(&mut cx, |bench, _, _| Rc::clone(&bench.draws))
        .unwrap();
    let (mut cpu, mut wall, mut allocs, mut renders, mut drawn) =
        (vec![], vec![], vec![], vec![], vec![]);
    for _ in 0..BATCHES {
        let (time, clock, count) = (cpu_ns(), Instant::now(), allocations());
        let (others, before) = (mark.counters.others.get(), draws.get());
        for _ in 0..FRAMES {
            frame(&mut cx, handle);
        }
        let frames = FRAMES as f64;
        cpu.push((cpu_ns() - time) as f64 / frames / 1000.);
        wall.push(clock.elapsed().as_secs_f64() * 1e6 / frames);
        allocs.push((allocations() - count) as f64 / frames);
        renders.push((mark.counters.others.get() - others) as f64 / frames);
        drawn.push((draws.get() - before) as f64 / frames);
    }
    println!(
        "SPINBENCH geometry={geometry:?} cached={cached} dual={dual} cpu_us_per_frame={} wall_us_per_frame={} allocs_per_frame={} shell_renders_per_frame={} draws_per_frame={}",
        range(cpu), range(wall), range(allocs), range(renders), range(drawn)
    );
}

#[test]
#[ignore]
fn arms() {
    for dual in [false, true] {
        for (geometry, cached) in [
            (Geometry::PathEveryFrame, false),
            (Geometry::Quads, false),
            (Geometry::PathCache, false),
            (Geometry::PathEveryFrame, true),
            (Geometry::PathCache, true),
            (Geometry::Quads, true),
        ] {
            measure(geometry, cached, dual);
        }
    }
}

#[test]
#[ignore]
fn hidden() {
    for timer in [true, false] {
        let mut cx = real_cx();
        let (handle, _keep) = open(&mut cx, true, Geometry::Quads, true, timer);
        let mark = loading(&mut cx, handle);
        for _ in 0..30 {
            frame(&mut cx, handle);
        }
        land(&mut cx, handle);
        let draws = handle
            .update(&mut cx, |bench, _, _| Rc::clone(&bench.draws))
            .unwrap();
        let (mut cpu, mut drawn, mut callbacks) = (vec![], vec![], vec![]);
        for _ in 0..BATCHES {
            let (time, before) = (cpu_ns(), draws.get());
            let ran: usize = (0..125).map(|_| frame(&mut cx, handle)).sum();
            cpu.push((cpu_ns() - time) as f64 / 1000.);
            drawn.push((draws.get() - before) as f64);
            callbacks.push(ran as f64);
        }
        assert!(!mark.shown());
        println!(
            "SPINBENCH hidden timer={timer} cpu_us_per_2s={} draws_per_2s={} frame_callbacks_per_2s={}",
            range(cpu), range(drawn), range(callbacks)
        );
    }
}
