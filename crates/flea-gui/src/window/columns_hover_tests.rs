//! Pointer moves must reach the focus snapshot actually used by the mounted track painter.
use super::host;
use crate::columns::geometry::{Slot, LANE};
use gpui::{point, px, Modifiers, VisualTestContext};
use std::rc::Rc;

#[test]
fn pb_columns1_hover_tracks_rows_and_clears_on_background_and_lane() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        host::peek(
            shell,
            "/srv/a/b",
            &[("c", true), ("other", false)],
            false,
            window,
            cx,
        );
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("item", false)],
            false,
            window,
            cx,
        );
    });
    host::frame(&mut cx, handle);
    let mut cx = VisualTestContext::from_window(handle.into(), &cx);
    for (slot, row) in [(Slot::Parent, 1), (Slot::Active, 1), (Slot::Child, 0)] {
        let bounds = handle
            .read_with(&cx, |shell, _| {
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .columns
                    .as_ref()
                    .unwrap()
                    .painters
                    .get(slot)
                    .unwrap()
                    .borrow()
                    .drawn
                    .as_ref()
                    .unwrap()
                    .bounds
            })
            .unwrap();
        let at = point(
            bounds.left() + px(70.),
            bounds.top() + px(row as f32 * 31. + 15.),
        );
        for (position, expected) in [
            (at, Some((slot, row))),
            (point(bounds.right() - px(LANE / 2.), at.y), None),
            (point(at.x, bounds.top() + px(300.)), None),
            (at, Some((slot, row))),
            (point(at.x, px(10.)), None),
        ] {
            cx.simulate_mouse_move(position, None, Modifiers::none());
            host::frame(&mut cx, handle);
            handle
                .read_with(&cx, |shell, _| {
                    let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
                    let paint = columns.painters.get(slot).unwrap().borrow();
                    let focus = &paint.drawn.as_ref().unwrap().focus;
                    assert_eq!(
                        focus.hover, expected,
                        "paint must receive the row under the pointer"
                    );
                    if expected.is_some() {
                        let palette = paint.painted.as_ref().unwrap();
                        assert_eq!(
                            focus.plate(palette, slot, row, false),
                            Some(palette.foreground.opacity(crate::columns::paint::HOVER))
                        );
                    }
                })
                .unwrap();
        }
    }
}

#[test]
fn pb_columns1_hover_follows_filtered_rows_and_clears_when_navigation_is_busy() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        host::settle(shell, window, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.filter.shown = Some(std::rc::Rc::from([1]));
    });
    host::frame(&mut cx, handle);
    let at = handle
        .read_with(&cx, |shell, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Active).unwrap().borrow();
            let bounds = paint.drawn.as_ref().unwrap().bounds;
            point(bounds.left() + px(70.), bounds.top() + px(15.))
        })
        .unwrap();
    let mut cx = VisualTestContext::from_window(handle.into(), &cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, window, cx| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        assert_eq!(
            columns
                .painters
                .get(Slot::Active)
                .unwrap()
                .borrow()
                .drawn
                .as_ref()
                .unwrap()
                .focus
                .hover,
            Some((Slot::Active, 1))
        );
        shell
            .step(
                0,
                crate::window::navigate::Step::OpenPlace("/next".into()),
                window,
                cx,
            )
            .unwrap();
    });
    host::frame(&mut cx, handle);
    handle
        .read_with(&cx, |shell, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            assert_eq!(
                columns
                    .painters
                    .get(Slot::Active)
                    .unwrap()
                    .borrow()
                    .drawn
                    .as_ref()
                    .unwrap()
                    .focus
                    .hover,
                None
            );
        })
        .unwrap();
}

#[test]
fn pb_columns1_hover_drops_when_a_still_pointer_lands_a_new_listing() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, handle);
    let at = handle
        .read_with(&cx, |shell, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Active).unwrap().borrow();
            let bounds = paint.drawn.as_ref().unwrap().bounds;
            point(bounds.left() + px(70.), bounds.top() + px(15.))
        })
        .unwrap();
    let mut cx = VisualTestContext::from_window(handle.into(), &cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    host::frame(&mut cx, handle);
    let hover = |cx: &mut VisualTestContext| {
        handle
            .read_with(&*cx, |shell, _| {
                let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
                columns
                    .painters
                    .get(Slot::Active)
                    .unwrap()
                    .borrow()
                    .drawn
                    .as_ref()
                    .unwrap()
                    .focus
                    .hover
            })
            .unwrap()
    };
    assert_eq!(hover(&mut cx), Some((Slot::Active, 0)));
    // The pointer stays still while the active listing is swapped for a newer one.
    host::change(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.held = Rc::new(crate::list::pane::Held {
            listing: host.pane.held.listing + 1,
            total: host.pane.held.total,
            start: host.pane.held.start,
            rows: host.pane.held.rows.clone(),
        });
    });
    host::frame(&mut cx, handle);
    assert_eq!(
        hover(&mut cx),
        None,
        "a listing that lands under a still pointer must not keep the old row's plate"
    );
}
