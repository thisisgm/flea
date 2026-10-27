//! A watched density change must invalidate a live band before row states are captured.
use super::store::Update;
use crate::{
    backend::Backend,
    input::Frame,
    list::{bench, pane::Held},
    presentation::Fonts,
    state::tests::shell_with,
    state::Shell,
    window::{
        pane::PaneHost,
        root::Root,
        startup::{Settings, View},
    },
};
use flea::{jsondoc, protocol::Reply};
use gpui::{
    point, px, size, AppContext, Context, Entity, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, Render, TestAppContext, Window, WindowHandle,
};
use std::rc::Rc;

type RenderCapture = Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> Frame>;

struct RenderPhaseRoot {
    shell: Entity<Shell>,
    mounted: Entity<Root>,
    capture: Option<RenderCapture>,
    prepared: Option<Frame>,
}

impl Render for RenderPhaseRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(capture) = self.capture.take() {
            assert_eq!(
                window.current_view(),
                cx.entity_id(),
                "capture must run in GPUI registered view layout"
            );
            println!("G76_RENDER_PHASE_CAPTURE");
            self.prepared = Some(
                self.shell
                    .update(cx, |shell, cx| capture(shell, window, cx)),
            );
        }
        self.mounted.clone()
    }
}

const DENSITIES: [&str; 4] = ["compact", "normal", "tight", "comfortable"];

fn open(cx: &mut TestAppContext, view: View, density: &str) -> WindowHandle<RenderPhaseRoot> {
    cx.update(|cx| {
        cx.set_global(Fonts::from_sources(
            "[font]\nbase-size=14",
            "",
            "JetBrains Mono".into(),
        ))
    });
    let (backend, _) = Backend::test_channel();
    let handle = cx.open_window(size(px(1100.), px(700.)), |_, cx| {
        let shell = cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            let patch = jsondoc::parse(&format!(r#"{{"density":"{density}"}}"#)).unwrap();
            let data = flea::uistate::patched(&flea::uistate::from_file("{}"), &patch).unwrap();
            shell.launch.settings = Settings::from_json(data);
            shell.view = view;
            let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
            right
                .pane
                .receive(Reply::Listed {
                    path: "/right".into(),
                    total: 8,
                })
                .unwrap();
            right
                .pane
                .receive(Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: bench::fixture("2k").into_iter().take(8).collect(),
                })
                .unwrap();
            shell.panes[1] = Some(right);
            shell.lifecycle.attach(1);
            for host in shell.panes.iter_mut().flatten() {
                host.backend = None;
                host.pane.held = Rc::new(Held {
                    listing: host.pane.held.listing,
                    total: 8,
                    start: 0,
                    rows: bench::fixture("2k").into_iter().take(8).collect(),
                });
                host.selected = vec![1];
                host.pane.cursor = 2;
                host.input.selection.changed(&host.selected);
                assert!(!host.pane.busy());
            }
            shell
        });
        let mounted = cx.new(|_| Root {
            shell: shell.clone(),
        });
        RenderPhaseRoot {
            shell,
            mounted,
            capture: None,
            prepared: None,
        }
    });
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.apply_settings(shell.launch.settings.data.clone(), window, cx);
            })
        })
        .unwrap();
    draw(cx, handle);
    handle
}

fn draw(cx: &mut TestAppContext, handle: WindowHandle<RenderPhaseRoot>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn gesture(cx: &mut TestAppContext, handle: WindowHandle<RenderPhaseRoot>, side: usize) -> Frame {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                assert!(!shell.input_blocked(cx));
                let frame = shell.panes[side]
                    .as_ref()
                    .unwrap()
                    .input
                    .drawn
                    .expect("mounted input")
                    .frame;
                let blank = point(
                    frame.bounds.left() + px(60.),
                    frame.bounds.bottom() - px(20.),
                );
                assert!(
                    frame.row_at(blank).is_none(),
                    "start the actual pointer route in blank pane space"
                );
                shell.pointer_down(
                    side,
                    &MouseDownEvent {
                        position: blank,
                        button: MouseButton::Left,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                shell.pointer_move(
                    side,
                    &MouseMoveEvent {
                        position: point(
                            frame.bounds.left() + px(100.),
                            frame.bounds.top() - px(20.),
                        ),
                        pressed_button: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    true,
                    window,
                    cx,
                );
                let host = shell.panes[side].as_ref().unwrap();
                let band = host
                    .input
                    .band
                    .as_ref()
                    .expect("pointer route created a band");
                assert!(
                    band.active && band.direction < 0.,
                    "an active edge-selection band is required"
                );
                assert_eq!(band.before, vec![1]);
                assert_eq!(band.cursor, 2);
                assert_ne!(
                    host.selected, band.before,
                    "the active band must change selection"
                );
                frame
            })
        })
        .unwrap()
}

fn check(view: View, side: usize, from: &str, to: &str) {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx, view, from);
    let old = gesture(&mut cx, handle, side);
    handle.update(&mut cx, |root, window, cx| {
        assert!(root.capture.is_none() && root.prepared.is_none());
        let capture = root.shell.update(cx, |shell, cx| {
        let host = shell.panes[side].as_ref().unwrap();
        let held = Rc::clone(&host.pane.held);
        let requests = host.pane.list_requests;
        let face = host.list.borrow().typeface.clone();
        let old_row = host.list.borrow().file_metrics().row;
        let old_selection = host.selected.clone();
        let old_cursor = host.pane.cursor;
        let patch = jsondoc::parse(&format!(r#"{{"density":"{to}"}}"#)).unwrap();
        let document = flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
        shell.receive_settings(Update::Loaded { document, through: 0, saved: false }, window, cx);
        assert!(!shell.input_blocked(cx));
        assert!(shell.seam.viewport().is_none(), "use actual mounted window geometry");
        let host = shell.panes[side].as_ref().unwrap();
        assert!(host.input.band.as_ref().is_some_and(|band| band.active), "the watched update must reach preparation with the band still active");
        assert!(!host.pane.busy());
        assert_eq!(host.pane.list_requests, requests);
        assert!(Rc::ptr_eq(&held, &host.pane.held));
        assert!(host.list.borrow().typeface == face);
        let current = host.list.borrow().file_metrics();
        assert_eq!(host.pane.row_height(), current.row);
        assert_eq!(current.row != old_row, from != to);
        let from = from.to_owned();
        let to = to.to_owned();
        Box::new(move |shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>| {
        let geometry = shell.geometry(window);
        let heights = std::array::from_fn(|pane| geometry.content_height_for(pane, shell.shown_view(), shell.slots.strip_height_for(pane)));
        let lists = shell.prepare_lists(heights, Some(geometry.chrome.height), window, cx);
        let prepared = lists.prepared[side].expect("actual prepare_lists returned this side's frame");
        let host = shell.panes[side].as_ref().unwrap();
        if from != to {
            assert!(host.input.band.is_none(), "G76_DENSITY_BAND_CANCEL: view={view:?} side={side} from={from} to={to} old_row={old_row} current_row={} prepared_row={}", current.row, prepared.row);
            assert_eq!(host.selected, vec![1], "selection rolls back before RowStates capture");
            assert_eq!(host.pane.cursor, 2);
            let states = lists.states[side].as_ref().unwrap();
            assert_eq!(states.selected.as_deref(), Some(&[1][..]));
            assert_eq!(states.cursor, Some(2));
        } else {
            assert!(host.input.band.as_ref().is_some_and(|band| band.active), "G76_SAME_GEOMETRY_BAND_PRESERVED");
            assert_eq!(host.selected, old_selection);
            assert_eq!(host.pane.cursor, old_cursor);
        }
        if from != to || from == "compact" {
            assert_eq!(prepared.row, current.row, "prepared geometry must match current file row metrics");
            assert_eq!(prepared.content, 8. * current.row + current.header);
        }
        assert_eq!(prepared.bounds, old.bounds, "density must not alter viewport geometry");
        assert_eq!(prepared.listing, old.listing);
        assert_eq!(prepared.total, old.total);
        assert_eq!(prepared.lane, old.lane);
        assert_eq!(host.pane.scroll_y, 0.);
        prepared
        }) as RenderCapture
        });
        root.capture = Some(capture);
        cx.notify();
    }).unwrap();
    draw(&mut cx, handle);
    let prepared = handle
        .update(&mut cx, |root, _, _| {
            assert!(
                root.capture.is_none(),
                "the real request-layout render consumed the capture"
            );
            root.prepared
                .take()
                .expect("the mounted render phase ran actual prepare_lists")
        })
        .unwrap();
    handle
        .update(&mut cx, |root, _, cx| {
            root.shell.read_with(cx, |shell, _| {
                let host = shell.panes[side].as_ref().unwrap();
                let list = host.list.borrow();
                let mounted = host.input.drawn.expect("fresh mounted input paint").frame;
                assert_eq!(mounted.bounds, prepared.bounds);
                if from != to || from == "compact" {
                    assert_eq!(mounted.row, prepared.row);
                    assert_eq!(list.drawn.metrics.row, prepared.row);
                    assert_eq!(mounted.content, prepared.content);
                }
            })
        })
        .unwrap();
}

#[test]
fn g76_watched_density_change_cancels_active_band_before_row_states() {
    for from in DENSITIES {
        for to in DENSITIES {
            if from == to {
                continue;
            }
            for (view, side) in [(View::List, 0), (View::Dual, 0), (View::Dual, 1)] {
                check(view, side, from, to);
            }
        }
    }
}

#[test]
fn g76_same_density_geometry_preserves_active_band() {
    for density in DENSITIES {
        for (view, side) in [(View::List, 0), (View::Dual, 0), (View::Dual, 1)] {
            check(view, side, density, density);
        }
    }
}

#[path = "density_scroll_tests.rs"]
mod stale_scroll;
