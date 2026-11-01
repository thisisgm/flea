//! Live frame and input regressions for the mounted Grid's viewport, pointer mask and failure path.
use super::join_tests::{call, frame, key, open};
use crate::window::startup::View;
use flea::ipc::wire::Reply;
use gpui::{point, px, Modifiers, MouseButton, TestAppContext, VisualTestContext};

#[gpui::test]
fn u10_grid_pointer_uses_painted_slots_and_ignores_bare_cell_margins(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    let (slot, bare) = handle
        .update(cx, |shell, _, _| {
            let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
            let state = grid.state.borrow();
            let tile = state.drawn_row(2).unwrap();
            let at = state.bounds.origin;
            (
                at + point(
                    px(tile.slot.x + tile.slot.width / 2.),
                    px(tile.slot.y + tile.slot.height / 2.),
                ),
                at + point(px(tile.cell.x + 2.), px(tile.cell.y + 2.)),
            )
        })
        .unwrap();
    {
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_mouse_move(slot, None, Modifiers::default());
        visual.simulate_mouse_down(slot, MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_up(slot, MouseButton::Left, Modifiers::default());
    }
    frame(cx, handle);
    assert_eq!(call(cx, handle, "cursor", &[]), Reply::value("2"));
    assert_eq!(call(cx, handle, "selectedIndices", &[]), Reply::value("2"));
    assert_eq!(call(cx, handle, "rowHovered", &["2"]), Reply::value("true"));
    {
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_mouse_move(bare, None, Modifiers::default());
        visual.simulate_mouse_down(bare, MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_up(bare, MouseButton::Left, Modifiers::default());
    }
    frame(cx, handle);
    assert_eq!(call(cx, handle, "cursor", &[]), Reply::value("2"));
    assert_eq!(
        call(cx, handle, "rowHovered", &["2"]),
        Reply::value("false")
    );
}

#[gpui::test]
fn u10_grid_resize_reflows_painted_columns_and_caption_facts(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            let held = &mut shell.panes[0].as_mut().unwrap().pane.held;
            *held = std::rc::Rc::new(crate::pane::Held {
                listing: held.listing,
                total: held.total,
                start: held.start,
                rows: held.rows.clone(),
            });
            let held = std::rc::Rc::get_mut(held).unwrap();
            held.rows[0].name = "a-long_filename_for-two-lines.txt".into();
            shell.set_view(View::Grid, window, cx);
        })
        .unwrap();
    frame(cx, handle);
    let before = call(cx, handle, "gridColumns", &[]);
    let caption = call(cx, handle, "gridCaptionState", &["0"]);
    assert!(caption.stdout.contains("a-long_"));
    {
        let visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_resize(gpui::size(px(500.), px(400.)));
    }
    frame(cx, handle);
    assert_ne!(call(cx, handle, "gridColumns", &[]), before);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let grid = host.grid.as_ref().unwrap();
            let state = grid.state.borrow();
            let layout = state.layout.unwrap();
            assert_eq!(
                layout.columns,
                grid.view
                    .layout(super::Shown::All(host.pane.held.total))
                    .columns
            );
            assert!(
                layout.columns as f32 * layout.cell_width <= layout.width - layout.metrics.pad_x
            );
            assert!(state.drawn().len() <= layout.columns * (layout.visible_rows() + 1));
            assert!(
                host.list.borrow().visible().is_empty(),
                "Grid never renders the inactive List"
            );
        })
        .unwrap();
}

#[gpui::test]
fn u10_grid_nonready_surfaces_clear_the_previous_frame_readers(cx: &mut TestAppContext) {
    use crate::pane::states::Kind;
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    assert!(!call(cx, handle, "rowRect", &["0"]).stdout.trim().is_empty());
    handle
        .update(cx, |shell, _, cx| {
            shell.grid_scroll(134., cx);
        })
        .unwrap();
    frame(cx, handle);
    assert_ne!(call(cx, handle, "viewContentY", &[]), Reply::value("0"));
    for kind in [Kind::Empty, Kind::Error, Kind::Locked, Kind::Loading] {
        handle
            .update(cx, |shell, _, cx| {
                shell.panes[0].as_mut().unwrap().pane.state.kind = kind;
                cx.notify();
            })
            .unwrap();
        frame(cx, handle);
        assert_eq!(call(cx, handle, "rowRect", &["0"]), Reply::value(""));
        assert_eq!(call(cx, handle, "rowNameText", &["0"]), Reply::value(""));
        assert_eq!(
            call(cx, handle, "gridCaptionState", &["0"]),
            Reply::value("{}")
        );
        assert_eq!(
            call(cx, handle, "gridCaptionState", &["-1"]),
            Reply::value("{}")
        );
        assert_eq!(call(cx, handle, "viewContentY", &[]), Reply::value("0"));
    }
    handle
        .update(cx, |shell, _, cx| {
            shell.panes[0].as_mut().unwrap().pane.state.kind = Kind::Ready;
            cx.notify();
        })
        .unwrap();
    frame(cx, handle);
    assert!(!call(cx, handle, "rowRect", &["10"])
        .stdout
        .trim()
        .is_empty());
}

#[gpui::test]
fn u10_returning_to_list_reveals_the_grid_cursor_before_the_first_list_frame(
    cx: &mut TestAppContext,
) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    for _ in 0..3 {
        key(cx, handle, "pagedown");
    }
    handle
        .update(cx, |shell, window, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.pane.cursor > host.pane.visible_rows());
            assert_eq!(host.pane.scroll_y, 0., "Grid keeps its own scroll");
            shell.set_view(View::List, window, cx);
            assert!(shell.panes[0].as_ref().unwrap().pane.scroll_y > 0.);
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host
                .list
                .borrow()
                .visible()
                .iter()
                .any(|row| row.index == host.pane.cursor));
            let list = host.list.borrow();
            let row = list.drawn.metrics.row;
            let bottom = host.pane.cursor as f32 * row - host.pane.scroll_y + row;
            assert!(
                bottom <= f32::from(list.drawn.bounds.size.height),
                "the entire cursor row fits after the List header returns"
            );
        })
        .unwrap();
}

#[gpui::test]
fn u10_grid_hover_follows_stationary_pointer_after_scroll_and_clears_on_exit(
    cx: &mut TestAppContext,
) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    let (point, height, columns) = handle
        .update(cx, |shell, _, _| {
            let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
            let state = grid.state.borrow();
            let tile = state.drawn_row(0).unwrap();
            (
                state.bounds.origin
                    + point(
                        px(tile.slot.x + tile.slot.width / 2.),
                        px(tile.slot.y + tile.slot.height / 2.),
                    ),
                state.layout.unwrap().cell_height,
                state.layout.unwrap().columns,
            )
        })
        .unwrap();
    {
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_mouse_move(point, None, Modifiers::default());
    }
    frame(cx, handle);
    assert_eq!(call(cx, handle, "rowHovered", &["0"]), Reply::value("true"));
    handle
        .update(cx, |shell, _, cx| {
            assert!(shell.grid_scroll(height, cx));
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(
        call(cx, handle, "rowHovered", &["0"]),
        Reply::value("false")
    );
    assert_eq!(
        call(cx, handle, "rowHovered", &[&columns.to_string()]),
        Reply::value("true")
    );
    {
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_mouse_move(gpui::point(px(10.), px(10.)), None, Modifiers::default());
    }
    frame(cx, handle);
    assert_eq!(
        call(cx, handle, "rowHovered", &[&columns.to_string()]),
        Reply::value("false")
    );
}

#[gpui::test]
fn the_keyboard_quiets_the_grid_hover_until_the_pointer_really_moves(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    let at = handle
        .update(cx, |shell, _, _| {
            let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
            let state = grid.state.borrow();
            let tile = state.drawn_row(2).unwrap();
            state.bounds.origin
                + point(
                    px(tile.slot.x + tile.slot.width / 2.),
                    px(tile.slot.y + tile.slot.height / 2.),
                )
        })
        .unwrap();
    let hovered = |cx: &mut TestAppContext| call(cx, handle, "rowHovered", &["2"]);
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_move(
        at,
        None,
        Modifiers::default(),
    );
    frame(cx, handle);
    assert_eq!(hovered(cx), Reply::value("true"));
    key(cx, handle, "right");
    frame(cx, handle);
    assert_eq!(
        hovered(cx),
        Reply::value("false"),
        "the keyboard hides the resting pointer's wash (#322)"
    );
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_move(
        at,
        None,
        Modifiers::default(),
    );
    frame(cx, handle);
    assert_eq!(
        hovered(cx),
        Reply::value("false"),
        "a move to the same point is not a move"
    );
    let moved = at + point(px(2.), px(0.));
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_move(
        moved,
        None,
        Modifiers::default(),
    );
    frame(cx, handle);
    assert_eq!(
        hovered(cx),
        Reply::value("true"),
        "a real move brings the wash back"
    );
}
