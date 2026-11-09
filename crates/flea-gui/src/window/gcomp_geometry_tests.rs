//! Imported input policy must act on the actual mounted List before row states are painted.
use super::*;
use gpui::{point, px, MouseButton, MouseDownEvent, MouseMoveEvent};

#[test]
fn gcomp_batch_u03_busy_receive_rolls_back_live_band_before_the_real_root_paints() {
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    edit(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::List, window, cx)
    });
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, window, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![1, 3];
        host.input.selection.sync(&host.selected);
        host.pane.cursor = 2;
        let drawn = host.input.drawn.expect("actual input surface painted");
        assert_eq!(drawn.frame.bounds, host.list.borrow().drawn.bounds);
        let bounds = drawn.frame.bounds;
        let start = point(bounds.left() + px(70.), bounds.bottom() - px(3.));
        assert!(
            host.input.hit(start).is_none(),
            "band starts in empty viewport"
        );
        shell.pointer_down(
            0,
            &MouseDownEvent {
                position: start,
                button: MouseButton::Left,
                ..Default::default()
            },
            window,
            cx,
        );
        shell.pointer_move(
            0,
            &MouseMoveEvent {
                position: point(bounds.left() + px(20.), bounds.top() + px(5.)),
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            },
            true,
            window,
            cx,
        );
        let host = shell.panes[0].as_mut().unwrap();
        assert!(host.input.band.as_ref().is_some_and(|band| band.active));
        assert_ne!(host.selected, [1, 3], "real pointer gesture changed marks");
        assert!(host
            .pane
            .begin("/pending".into(), Default::default(), host.pane.now()));
        shell.receive(
            0,
            BackendEvent::Reply(flea::protocol::Reply::Listed {
                path: "/pending".into(),
                total: 20,
            }),
            window,
            cx,
        );
        assert!(shell.panes[0].as_ref().unwrap().pane.busy());
    });
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.input.band.is_none());
        assert!(host.input.timer.is_none());
        assert!(host.input.stepped_at.is_none());
        assert_eq!(host.selected, [1, 3]);
        assert_eq!(host.pane.cursor, 2);
        let list = host.list.borrow();
        for row in list.visible() {
            assert_eq!(row.state.selected, [1, 3].contains(&row.index));
        }
    });
}

#[test]
fn gcomp_fix3_mounted_dual_selection_uses_one_retained_snapshot_per_pane() {
    use std::rc::Rc;
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    // The focused pane draws focused only in the active window; a TestWindow starts inactive.
    edit(&mut cx, handle, |_, window, _| window.activate_window());
    cx.run_until_parked();
    let snapshots = edit(&mut cx, handle, |shell, _, _| {
        std::array::from_fn::<_, 2, _>(|side| {
            let host = shell.panes[side].as_mut().unwrap();
            host.selected = if side == 0 { vec![1, 2, 3] } else { vec![7] };
            host.input.selection.sync(&host.selected);
            host.pane.cursor = 5 + side;
            host.input.hovered = Some(4 - side);
            host.input.pointer_origin = true;
            host.input.selection.snapshot.clone().unwrap()
        })
    });
    for focused in [0, 1, 0] {
        edit(&mut cx, handle, |shell, window, cx| {
            shell.focus_pane(focused, cx);
            window.focus(&shell.focus, cx);
            let geometry = shell.geometry(window);
            let viewport = shell.seam.viewport();
            let heights = std::array::from_fn(|side| {
                viewport.map_or_else(
                    || {
                        geometry.content_height_for(
                            side,
                            View::Dual,
                            shell.slots.strip_height_for(side),
                        )
                    },
                    |viewport| f32::from(viewport.height),
                )
            });
            let lists = shell.prepare_lists(
                heights,
                viewport.is_none().then_some(geometry.chrome.height),
                window,
                cx,
            );
            for (side, snapshot) in snapshots.iter().enumerate() {
                let host = shell.panes[side].as_ref().unwrap();
                let states = lists.states[side].as_ref().unwrap();
                assert!(Rc::ptr_eq(states.selected.as_ref().unwrap(), snapshot));
                assert_eq!(states.listing, host.pane.held.listing);
                assert_eq!(states.cursor, Some(5 + side));
                assert_eq!(states.focused, side == focused);
                assert_eq!(states.hovered, Some(4 - side));
            }
        });
        frame(&mut cx, handle);
        edit(&mut cx, handle, |shell, _, _| {
            for (side, snapshot) in snapshots.iter().enumerate() {
                let host = shell.panes[side].as_ref().unwrap();
                assert!(Rc::ptr_eq(
                    host.input.selection.snapshot.as_ref().unwrap(),
                    snapshot
                ));
                let list = host.list.borrow();
                assert!(Rc::ptr_eq(&list.drawn_rows, &host.pane.held));
                assert_eq!(list.drawn_focused, side == focused);
                assert_eq!(list.visible().len(), 8, "actual mounted rows were painted");
                for row in list.visible() {
                    assert_eq!(row.state.selected, snapshot.contains(&row.index));
                    assert_eq!(row.state.cursor, row.index == 5 + side);
                    assert_eq!(row.state.hovered, row.index == 4 - side);
                }
            }
        });
    }
    edit(&mut cx, handle, |shell, _, _| {
        let left = shell.panes[0].as_mut().unwrap();
        left.input.selection.only(&mut left.selected, 5);
        assert!(!Rc::ptr_eq(
            left.input.selection.snapshot.as_ref().unwrap(),
            &snapshots[0]
        ));
    });
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, _, _| {
        for (side, expected) in [(0, 5), (1, 7)] {
            let host = shell.panes[side].as_ref().unwrap();
            for row in host.list.borrow().visible() {
                assert_eq!(row.state.selected, row.index == expected);
            }
        }
        let left = shell.panes[0].as_mut().unwrap();
        left.input.selection.clear(&mut left.selected);
    });
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, window, cx| {
        let left = shell.panes[0].as_ref().unwrap();
        assert!(left.input.selection.snapshot.is_none());
        assert!(left
            .list
            .borrow()
            .visible()
            .iter()
            .all(|row| !row.state.selected));
        let right = shell.panes[1].as_mut().unwrap();
        assert!(Rc::ptr_eq(
            right.input.selection.snapshot.as_ref().unwrap(),
            &snapshots[1]
        ));
        let pane = &mut right.pane;
        assert!(pane.begin("/replacement".into(), Default::default(), pane.now()));
        for reply in [
            flea::protocol::Reply::Listed {
                path: "/replacement".into(),
                total: 8,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                listing: 42,
                rows: vec![Default::default(); 8],
            },
        ] {
            shell.receive(1, BackendEvent::Reply(reply), window, cx);
        }
    });
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[1].as_ref().unwrap();
        assert_eq!(host.pane.held.listing, 42);
        assert!(host.selected.is_empty());
        assert!(host.input.selection.snapshot.is_none());
        let list = host.list.borrow();
        assert!(Rc::ptr_eq(&list.drawn_rows, &host.pane.held));
        assert_eq!(list.visible().len(), 8);
        assert!(list
            .visible()
            .iter()
            .all(|row| !row.state.selected && !row.state.hovered));
    });
}

struct GeometryStrip;

impl gpui::Render for GeometryStrip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        gpui::div()
    }
}

#[test]
fn gcomp_g70_asymmetric_strips_match_prepared_hits_viewports_and_scroll_extent() {
    use std::rc::Rc;
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    edit(&mut cx, handle, |shell, _, _| {
        for host in shell.panes.iter_mut().flatten() {
            host.pane.held = Rc::new(crate::list::pane::Held {
                listing: host.pane.held.listing,
                total: 128,
                start: 0,
                rows: (0..128)
                    .map(|at| flea::protocol::RowFacts {
                        name: format!("strip-row-{at}.txt"),
                        ..Default::default()
                    })
                    .collect(),
            });
        }
    });
    for (view, mounted, strip_height) in [
        (View::Dual, [false, false], 0.),
        (View::Dual, [true, false], 30.),
        (View::Dual, [false, true], 30.),
        (View::Dual, [true, true], 30.),
        (View::Dual, [false, false], 30.),
        (View::List, [false, true], 30.),
        (View::List, [true, false], 30.),
        (View::List, [false, false], 0.),
    ] {
        edit(&mut cx, handle, |shell, window, cx| {
            shell.set_view(view, window, cx);
            shell.slots.strip_height = strip_height;
            shell.slots.strips =
                mounted.map(|mounted| mounted.then(|| cx.new(|_| GeometryStrip).into()));
            for host in shell.panes.iter_mut().flatten() {
                host.pane.scroll_to(0.);
            }
            cx.notify();
        });
        frame(&mut cx, handle);
        edit(&mut cx, handle, |shell, window, cx| {
            let count = if view == View::Dual { 2 } else { 1 };
            for (side, &mounted) in mounted.iter().enumerate().take(count) {
                let host = shell.panes[side].as_mut().unwrap();
                let painted = host
                    .input
                    .drawn
                    .expect("actual mounted input painted")
                    .frame;
                let list = host.list.borrow();
                assert_eq!(painted.bounds, list.drawn.bounds);
                let strip = if mounted { strip_height } else { 0. };
                assert_eq!(
                    painted.bounds.top(),
                    list.header.drawn.bounds.bottom() + px(strip),
                    "painted body follows only its own mounted strip"
                );
                assert_eq!(host.pane.scroll_extent, Some(painted.content));
                host.pane.scroll_to(f32::MAX);
                assert!(
                    (host.pane.scroll_y - painted.max_scroll()).abs() < 0.01,
                    "render prepared a different viewport for side {side}: mounted={mounted}"
                );
                host.pane.scroll_to(0.);
            }
            let geometry = shell.geometry(window);
            assert!(
                shell.seam.viewport().is_none(),
                "exercise actual pane composition"
            );
            let heights = std::array::from_fn(|side| {
                geometry.content_height_for(side, view, shell.slots.strip_height_for(side))
            });
            let lists = shell.prepare_lists(heights, Some(geometry.chrome.height), window, cx);
            for side in 0..count {
                let host = shell.panes[side].as_ref().unwrap();
                let painted = host.input.drawn.unwrap().frame;
                let prepared = lists.prepared[side].expect("actual frame passed to input.prepare");
                assert_eq!(
                    prepared.bounds, painted.bounds,
                    "prepared and painted bounds differ for side {side}, strips={mounted:?}"
                );
                assert_eq!(prepared.viewport(), painted.viewport());
                assert_eq!(prepared.max_scroll(), painted.max_scroll());
                let first = point(
                    painted.bounds.left() + px(20.),
                    painted.bounds.top() + px(painted.row / 2.),
                );
                assert_eq!(prepared.row_at(first), Some(0));
                assert_eq!(host.input.hit(first), Some(0));
                for y in [
                    painted.bounds.top() - px(1.),
                    painted.bounds.bottom() + px(1.),
                ] {
                    let outside = point(first.x, y);
                    assert_eq!(prepared.row_at(outside), None);
                    assert_eq!(host.input.hit(outside), None);
                }
                assert!(
                    prepared.max_scroll() > prepared.viewport(),
                    "fixture has a meaningful scroll extent"
                );
            }
        });
    }
}
