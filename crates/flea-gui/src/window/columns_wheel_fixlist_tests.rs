//! FIX-LIST 1101: a still pointer re-hovers what a Columns wheel or edge drag moved under it, and a long column ends on its footer band.
use super::*;
use crate::list::pane::ROW_HEIGHT;

fn drawn_hover(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    slot: Slot,
    at: Point<Pixels>,
) -> (Option<(Slot, usize)>, Option<usize>, f32) {
    let mut out = None;
    host::change(cx, handle, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let paint = columns.painters.get(slot).unwrap().borrow();
        let drawn = paint.drawn.as_ref().unwrap();
        let position = ((f32::from(at.y - drawn.bounds.top()) + drawn.scroll) / drawn.row_height)
            .floor() as usize;
        out = Some((drawn.focus.hover, drawn.row_at(position), drawn.scroll));
    });
    out.unwrap()
}

/// EY-117: a wheel on a track under a still pointer hovers the row it brings there, as v0.3.12 does, with no new motion.
#[test]
fn fixlist_ey117_columns_hover_follows_a_wheel_under_a_still_pointer() {
    for slot in [Slot::Active, Slot::Parent] {
        let mut cx = host::context();
        cx.update(|cx| cx.set_reduce_motion(true));
        let handle = mounted(&mut cx);
        let at = inside(&mut cx, handle, slot);
        VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
            at,
            None,
            gpui::Modifiers::none(),
        );
        host::frame(&mut cx, handle);
        let (hover, under, _) = drawn_hover(&mut cx, handle, slot, at);
        assert_eq!(
            hover,
            under.map(|row| (slot, row)),
            "{slot:?}: the pointer hovers its row"
        );
        send(
            &mut cx,
            handle,
            at,
            ScrollDelta::Lines(point(0., -1.)),
            TouchPhase::Moved,
        );
        VisualTestContext::from_window(handle.into(), &cx).run_until_parked();
        for _ in 0..3 {
            host::frame(&mut cx, handle);
        }
        let (hover, under, scroll) = drawn_hover(&mut cx, handle, slot, at);
        assert!(scroll > 0., "{slot:?}: the wheel scrolled the track");
        assert!(under.is_some(), "{slot:?}: a row sits under the pointer");
        assert_eq!(
            hover,
            under.map(|row| (slot, row)),
            "{slot:?}: the row now under the pointer takes the hover"
        );
    }
}

/// EY-117: an edge drag's release stores where the still pointer rests, so the next frame re-reads the hover there.
#[test]
fn fixlist_ey117_columns_edge_release_keeps_the_resting_pointer() {
    let mut cx = host::context();
    let handle = mounted(&mut cx);
    let at = {
        let mut at = None;
        host::change(&mut cx, handle, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Active).unwrap().borrow();
            let bounds = paint.drawn.as_ref().unwrap().bounds;
            at = Some(point(
                bounds.left()
                    + px(columns.area.layout.edge() - columns.area.layout.active_x() - 1.),
                bounds.top() + px(3. * ROW_HEIGHT + 15.),
            ));
        });
        at.unwrap()
    };
    let mut before = 0.;
    host::change(&mut cx, handle, |shell, _, _| {
        before = shell.panes[0]
            .as_ref()
            .unwrap()
            .columns
            .as_ref()
            .unwrap()
            .area
            .share;
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(at, None, gpui::Modifiers::none());
    vcx.run_until_parked();
    vcx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    let released = at + point(px(-120.), px(0.));
    vcx.simulate_mouse_move(
        released,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::none(),
    );
    vcx.simulate_mouse_up(released, gpui::MouseButton::Left, gpui::Modifiers::none());
    vcx.run_until_parked();
    host::change(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let columns = host.columns.as_ref().unwrap();
        assert!(!columns.area.host.edge.dragging, "the drag ended");
        assert_ne!(columns.area.share, before, "the drag resized the tracks");
        assert_eq!(
            host.input.resting(),
            Some(released),
            "the release left no resting pointer to re-read the hover from"
        );
    });
}

/// EY-117: a frame re-reads the hover from a resting pointer that no move event brought to its row.
#[test]
fn fixlist_ey117_columns_frame_rehovers_from_the_resting_pointer() {
    let mut cx = host::context();
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = mounted(&mut cx);
    let moved = inside(&mut cx, handle, Slot::Active);
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
        moved,
        None,
        gpui::Modifiers::none(),
    );
    host::frame(&mut cx, handle);
    let rested = moved + point(px(0.), px(3. * ROW_HEIGHT));
    host::change(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        assert!(!host.pane.busy(), "the fixture pane is settled");
        // What an edge release leaves behind: the pointer rests where no move event reported it.
        host.input.last_pointer = Some(rested);
        host.input.pointer_origin = true;
    });
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let under = crate::window::columns::hover_at(host, rested);
        assert!(under.is_some(), "a row sits under the resting pointer");
        assert_ne!(under, crate::window::columns::hover_at(host, moved));
        assert_eq!(
            host.columns.as_ref().unwrap().hover,
            under,
            "the frame hovers the row under the resting pointer"
        );
    });
}

/// EY-525: a long column wheeled to its end keeps ColumnPane.qml's footer band below its last row, on every track.
#[test]
fn fixlist_ey525_a_long_column_ends_on_its_footer_band() {
    for slot in [Slot::Parent, Slot::Active] {
        let mut cx = host::context();
        cx.update(|cx| cx.set_reduce_motion(true));
        let handle = mounted(&mut cx);
        let at = inside(&mut cx, handle, slot);
        for _ in 0..60 {
            send(
                &mut cx,
                handle,
                at,
                ScrollDelta::Lines(point(0., -3.)),
                TouchPhase::Moved,
            );
            VisualTestContext::from_window(handle.into(), &cx).run_until_parked();
            host::frame(&mut cx, handle);
        }
        let mut seen = None;
        host::change(&mut cx, handle, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(slot).unwrap().borrow();
            let drawn = paint.drawn.as_ref().unwrap();
            let viewport = f32::from(drawn.bounds.size.height);
            seen = Some((drawn.scroll, viewport, drawn.row_height));
        });
        let (scroll, viewport, row) = seen.unwrap();
        assert_eq!(
            scroll,
            TOTAL as f32 * row + crate::columns::geometry::FOOTER - viewport,
            "{slot:?}: the end leaves the footer band under the last row"
        );
    }
}
