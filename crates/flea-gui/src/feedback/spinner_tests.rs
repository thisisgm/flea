use super::spinner::{phase, Crawl, HERO, TRAVEL};
use crate::{
    backend::{Backend, Event as BackendEvent},
    pane::swap::{Options, CAP_MS, MIN_SHOW_MS},
    state::{tests::shell_with, Shell},
    window::{root::Root, startup::View},
};
use flea::protocol::{Reply, RowFacts};
use gpui::{
    point, px, size, AppContext, Bounds, Context, Entity, Render, ScaledPixels, TestAppContext,
    Window, WindowHandle,
};
use std::{cell::Cell, rc::Rc, time::Duration};

pub(super) const FRAME: Duration = Duration::from_millis(16);

/// The backend ends a test keeps alive, so a listing request is sent rather than refused.
pub(super) type Requests = Vec<Box<dyn FnMut() -> Option<(usize, usize)>>>;

/// A window root that holds the shell, so these helpers drive the shipped root and the bench arms alike.
pub(super) trait Holds: Render {
    fn held(&self) -> &Entity<Shell>;
}

impl Holds for Root {
    fn held(&self) -> &Entity<Shell> {
        &self.shell
    }
}

/// A window whose root `root` builds around a shell with one pane, or two in the dual view.
pub(super) fn open_with<V: Holds>(
    cx: &mut TestAppContext,
    dual: bool,
    root: impl FnOnce(Entity<Shell>, &mut Window, &mut Context<V>) -> V,
) -> (WindowHandle<V>, Requests) {
    let (backend, first) = Backend::test_channel();
    let (spare, second) = Backend::test_channel();
    let handle = cx.add_window(|window, cx| {
        let shell = cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            if dual {
                shell.panes[1] = shell_with(spare, cx).panes[0].take();
                shell.view = View::Dual;
            }
            shell
        });
        root(shell, window, cx)
    });
    (handle, vec![Box::new(first), Box::new(second)])
}

pub(super) fn open(cx: &mut TestAppContext, dual: bool) -> (WindowHandle<Root>, Requests) {
    open_with(cx, dual, |shell, _, _| Root { shell })
}

pub(super) fn shell<V: Holds, R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<V>,
    update: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.held()
                .clone()
                .update(cx, |shell, cx| update(shell, window, cx))
        })
        .unwrap()
}

/// Starts a listing on side 0 and spends its hold, so the pane falls back to the loading state.
pub(super) fn load<V: Holds>(cx: &mut TestAppContext, handle: WindowHandle<V>) -> Rc<Crawl> {
    load_side(cx, handle, 0)
}

fn load_side<V: Holds>(cx: &mut TestAppContext, handle: WindowHandle<V>, side: usize) -> Rc<Crawl> {
    shell(cx, handle, |shell, window, cx| {
        assert!(shell.navigate(side, "/slow".into(), None, Options::default(), window, cx));
        let host = shell.panes[side].as_mut().unwrap();
        assert!(host.pane.tick(host.pane.now() + CAP_MS));
        host.changed(&shell.locale);
        cx.notify();
        Rc::clone(&host.pane.swap.mark)
    })
}

/// The listing lands past the mark's minimum within one platform frame, whose due callbacks run before it draws.
pub(super) fn land<V: Holds>(cx: &mut TestAppContext, handle: WindowHandle<V>) {
    handle
        .update(cx, |root, window, cx| {
            root.held().clone().update(cx, |shell, cx| {
                let listed = Reply::Listed {
                    path: "/slow".into(),
                    total: 2,
                };
                shell.receive(0, BackendEvent::Reply(listed), window, cx);
                let rows = Reply::Rows {
                    start: 0,
                    rows: vec![RowFacts::default(); 2],
                    listing: 3,
                };
                shell.receive(0, BackendEvent::Reply(rows), window, cx);
                let host = shell.panes[0].as_mut().unwrap();
                host.pane.tick(host.pane.now() + MIN_SHOW_MS);
                host.changed(&shell.locale);
                assert!(!host.pane.busy());
                cx.notify();
            });
            window.simulate_next_frame(cx);
        })
        .unwrap();
}

/// One platform frame 16 ms on: due next-frame callbacks run, and the window draws only if that dirtied it.
pub(super) fn frame<V: 'static>(cx: &mut TestAppContext, handle: WindowHandle<V>) -> usize {
    cx.executor().advance_clock(FRAME);
    cx.update_window(handle.into(), |_, window, cx| {
        window.simulate_next_frame(cx)
    })
    .unwrap()
}

/// Window frames drawn while side 0 showed its loading state, counted by the root's probe.
pub(super) fn drawn(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> usize {
    shell(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .swap
            .frames
            .loading
            .get()
    })
}

fn notices<T: 'static>(
    cx: &mut TestAppContext,
    entity: &Entity<T>,
) -> (Rc<Cell<usize>>, gpui::Subscription) {
    let count = Rc::new(Cell::new(0));
    let seen = Rc::clone(&count);
    let subscription = cx.update(|cx| cx.observe(entity, move |_, _| seen.set(seen.get() + 1)));
    (count, subscription)
}

#[gpui::test]
fn the_crawl_moves_on_the_second_frame_after_show(cx: &mut TestAppContext) {
    let (handle, _keep) = open(cx, false);
    let mark = load(cx, handle);
    assert_eq!(
        (drawn(cx, handle), mark.shown(), mark.offset()),
        (1, true, 0.)
    );
    assert_eq!(frame(cx, handle), 1);
    assert_eq!(drawn(cx, handle), 2);
    assert!(mark.offset() < 0.);
    assert_eq!(mark.offset(), -TRAVEL * phase(FRAME));
}

#[gpui::test]
fn every_drawn_frame_writes_the_offset_once_and_asks_for_one_more(cx: &mut TestAppContext) {
    let (handle, _keep) = open(cx, true);
    let mark = load(cx, handle);
    for frames in 1..=120 {
        if frames > 1 {
            assert_eq!(frame(cx, handle), 1);
        }
        let counters = &mark.counters;
        let counted = (
            counters.frames.get(),
            counters.writes.get(),
            counters.requests.get(),
        );
        assert_eq!(
            (drawn(cx, handle), counted),
            (frames, (frames, frames, frames))
        );
    }
    assert!(mark.offset() < 0. && mark.offset() > -TRAVEL);
}

#[gpui::test]
fn a_crawl_frame_renders_no_other_view(cx: &mut TestAppContext) {
    let (handle, _keep) = open(cx, true);
    let mark = load(cx, handle);
    let entity = handle.update(cx, |root, _, _| root.shell.clone()).unwrap();
    let (notified, _subscription) = notices(cx, &entity);
    for _ in 0..120 {
        assert_eq!(frame(cx, handle), 1);
    }
    assert_eq!(drawn(cx, handle), 121);
    assert_eq!((mark.counters.others.get(), notified.get()), (0, 0));
}

#[gpui::test]
fn nothing_asks_for_a_frame_while_the_mark_is_hidden(cx: &mut TestAppContext) {
    let (handle, _keep) = open(cx, false);
    let mark = load(cx, handle);
    for _ in 0..30 {
        frame(cx, handle);
    }
    land(cx, handle);
    let (root, entity) = handle
        .update(cx, |root, _, cx| (cx.entity(), root.shell.clone()))
        .unwrap();
    let (roots, _root) = notices(cx, &root);
    let (shells, _shell) = notices(cx, &entity);
    let counted = (mark.counters.frames.get(), mark.counters.requests.get());
    for _ in 0..125 {
        assert_eq!(frame(cx, handle), 0);
    }
    cx.run_until_parked();
    assert!(!mark.shown());
    assert_eq!((roots.get(), shells.get()), (0, 0));
    assert_eq!(
        (mark.counters.frames.get(), mark.counters.requests.get()),
        counted
    );
}

#[gpui::test]
fn a_hide_puts_the_dash_back_to_rest_and_the_next_show_starts_there(cx: &mut TestAppContext) {
    let (handle, _keep) = open(cx, false);
    let mark = load(cx, handle);
    for _ in 0..50 {
        frame(cx, handle);
    }
    assert!(mark.offset() < -1.);
    land(cx, handle);
    assert_eq!((mark.shown(), mark.offset()), (false, 0.));
    load(cx, handle);
    assert_eq!((mark.shown(), mark.offset()), (true, 0.));
    frame(cx, handle);
    assert_eq!(mark.offset(), -TRAVEL * phase(FRAME));
}

#[gpui::test]
fn reduced_motion_draws_the_mark_at_rest_and_asks_for_no_frames(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (handle, _keep) = open(cx, false);
    let mark = load(cx, handle);
    assert_eq!(frame(cx, handle), 0);
    let counters = &mark.counters;
    let counted = (
        counters.frames.get(),
        counters.requests.get(),
        mark.offset(),
    );
    assert_eq!((drawn(cx, handle), counted), (1, (1, 0, 0.)));
}

#[gpui::test]
fn the_pane_describe_reports_the_marks_counters(cx: &mut TestAppContext) {
    use crate::ipc::{pane::PaneAdapter, Adapter};
    let (handle, _keep) = open(cx, false);
    let mark = load(cx, handle);
    for _ in 0..9 {
        frame(cx, handle);
    }
    let reply = shell(cx, handle, |shell, _, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        let adapter = PaneAdapter {
            pane,
            catalog: &shell.locale.catalog,
        };
        adapter.call("swapState", &[]).unwrap()
    });
    let data = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
    let described = data.get("mark").unwrap();
    let count = |key: &str| described.get(key).and_then(flea::jsondoc::Json::as_f64);
    assert_eq!(
        described
            .get("shown")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
    for key in ["frames", "writes", "requests"] {
        assert_eq!(count(key), Some(10.), "{key}");
    }
    assert_eq!(count("others"), Some(0.));
    assert!((count("offset").unwrap() - f64::from(mark.offset())).abs() < 0.001);
}

#[gpui::test]
fn a_narrow_dual_pane_clips_its_mark_at_the_pane_edge(cx: &mut TestAppContext) {
    for side in 0..2 {
        let (handle, _keep) = open(cx, true);
        // A 192 px rail, two 30 px panes and the 1 px between them make each pane narrower than the centred 48 px mark.
        cx.simulate_window_resize(handle.into(), size(px(253.), px(400.)));
        let mark = load_side(cx, handle, side);
        let mut past = false;
        for frames in 0..25 {
            if frames > 0 {
                assert_eq!(frame(cx, handle), 1);
            }
            let (pane, width, square, quads) = shell(cx, handle, |shell, window, _| {
                let pane = shell.geometry(window).panes[side];
                let scale = window.scale_factor();
                let edges = Bounds::new(
                    point(px(pane.x), px(pane.y)),
                    size(px(pane.width), px(pane.height)),
                );
                let square = mark.slot().scale(scale);
                (
                    edges.scale(scale),
                    pane.width,
                    square,
                    window.painted_quads(),
                )
            });
            assert!(width < HERO, "side {side}: pane {width} px");
            let ink = mark.ink();
            let ours: Vec<_> = quads
                .iter()
                .filter(|quad| {
                    quad.background
                        .as_solid()
                        .is_some_and(|color| (color.h, color.s, color.l) == (ink.h, ink.s, ink.l))
                        && quad.bounds.is_contained_within(&square)
                })
                .collect();
            assert!(!ours.is_empty(), "side {side} frame {frames}: no mark quad");
            for quad in ours {
                let shown = quad.bounds.intersect(&quad.content_mask.bounds);
                assert!(
                    inside(shown, pane),
                    "side {side} frame {frames}: {shown:?} leaves the pane {pane:?}"
                );
                past |= !inside(quad.bounds, pane);
            }
        }
        assert!(past, "side {side}: the mark never reached past its pane");
    }
}

fn inside(bounds: Bounds<ScaledPixels>, pane: Bounds<ScaledPixels>) -> bool {
    let near = |a: ScaledPixels, b: ScaledPixels| a.0 >= b.0 - 0.01;
    near(bounds.origin.x, pane.origin.x)
        && near(bounds.origin.y, pane.origin.y)
        && near(pane.right(), bounds.right())
        && near(pane.bottom(), bounds.bottom())
}
