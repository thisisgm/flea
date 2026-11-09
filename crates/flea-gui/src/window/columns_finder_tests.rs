//! Finder rule 5 in Columns: a right click opens the menu for its row, or the selection it is in, and moves nothing.
use super::host;
use crate::columns::geometry::Slot;
use gpui::{point, px, MouseButton, MouseDownEvent};

fn right_click(
    shell: &mut crate::state::Shell,
    slot: Slot,
    row: usize,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::state::Shell>,
) {
    let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
    let paint = columns.painters.get(slot).unwrap().borrow();
    let drawn = paint.drawn.as_ref().unwrap();
    let position = point(
        drawn.bounds.origin.x + drawn.bounds.size.width / 2.,
        drawn.bounds.origin.y + px((row as f32 + 0.5) * crate::list::pane::ROW_HEIGHT),
    );
    drop(paint);
    shell.columns_pointer(
        0,
        &MouseDownEvent {
            position,
            button: MouseButton::Right,
            click_count: 1,
            ..Default::default()
        },
        window,
        cx,
    );
}

#[test]
fn finder_active_right_click_keeps_cursor_and_selection_and_targets_its_row_or_selection() {
    // (selected before, row clicked, rows the menu takes)
    for (selected, clicked, rows) in [
        (vec![], 1, vec![1]),
        (vec![0], 1, vec![1]),
        (vec![0, 1], 1, vec![0, 1]),
    ] {
        let mut cx = host::context();
        let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
        host::change(&mut cx, handle, |shell, window, cx| {
            host::settle(shell, window, cx);
            shell.panes[0].as_mut().unwrap().selected = selected.clone();
        });
        host::frame(&mut cx, handle);
        host::change(&mut cx, handle, |shell, window, cx| {
            right_click(shell, Slot::Active, clicked, window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.place(), "/srv/a/b/c", "the columns moved");
            assert_eq!(host.nav.counters.lists, 0, "a right click listed a folder");
            assert_eq!(host.pane.cursor, 0, "a right click moved the cursor");
            assert_eq!(
                host.selected, selected,
                "a right click changed the selection"
            );
            let captured = shell.menu.captured.as_ref().expect("the menu opened");
            assert_eq!(captured.rows, rows);
            assert_eq!(captured.cursor, clicked);
            assert!(!captured.background);
            // S2-6: the ring marks a lone target the cursor is not on; a selection block keeps its own plates.
            let ring = (rows == [clicked]).then_some((Slot::Active, clicked));
            assert_eq!(shell.column_menu_focus(0).0, ring);
            shell.retire_menu(window, cx);
            assert_eq!(
                shell.column_menu_focus(0).0,
                None,
                "the ring outlived the menu"
            );
        });
    }
}

#[test]
fn a_keyboard_menu_on_the_cursor_row_draws_no_ring() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        host::settle(shell, window, cx)
    });
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, window, cx| {
        shell.open_menu(0, Some(0), (10., 10.), window, cx);
        // Every other guard holds, so only the cursor test keeps the ring away.
        let captured = shell.menu.captured.as_ref().expect("the menu opened");
        assert!(captured.target.is_none() && !captured.background);
        assert_eq!(captured.rows, [0]);
        assert_eq!(
            captured.cursor,
            shell.panes[0].as_ref().unwrap().pane.cursor
        );
        assert_eq!(shell.column_menu_focus(0).0, None);
    });
}

#[test]
fn finder_menu_row_actions_adopt_the_right_clicked_target() {
    use crate::{g2::context_menus::join::Chosen, window::dispatch::Order};
    use std::{cell::Cell, rc::Rc};
    for action in ["rename", "open", "run"] {
        let mut cx = host::context();
        let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
        host::change(&mut cx, handle, |shell, window, cx| {
            host::settle(shell, window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            host.pane
                .receive(flea::protocol::Reply::Rows {
                    start: 0,
                    listing: host.pane.held.listing,
                    rows: vec![host::row("a.txt", false), host::row("b.txt", false)],
                })
                .unwrap();
            host.selected = vec![0];
        });
        host::frame(&mut cx, handle);
        host::change(&mut cx, handle, |shell, window, cx| {
            right_click(shell, Slot::Active, 1, window, cx);
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 0);
            let mut captured = shell.menu.captured.clone().unwrap();
            captured.ready = true;
            let claimed = Rc::new(Cell::new(false));
            shell.menu.context.row_is_file = true;
            shell.dispatch(
                Order::Menu(Chosen {
                    captured,
                    action: action.into(),
                    leaf: String::new(),
                    paths: vec!["/srv/a/b/c/b.txt".into()],
                    claimed: claimed.clone(),
                }),
                window,
                cx,
            );
            assert!(claimed.get(), "{action} did not claim the menu choice");
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, 1, "{action} ignored the clicked target");
            assert!(host.selected.is_empty(), "{action} kept the old selection");
        });
    }
}

fn places_width(shell: &crate::state::Shell) -> Option<f64> {
    shell
        .launch
        .settings
        .data
        .get("places")
        .and_then(|places| places.get("sidebarWidth"))
        .and_then(flea::jsondoc::Json::as_f64)
}

#[test]
fn finder_mounted_column_hairline_drags_its_track_and_stores_the_tracks() {
    use gpui::{Modifiers, VisualTestContext};
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, host::settle);
    host::frame(&mut cx, handle);
    let (at, before) = handle
        .update(&mut cx, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Parent).unwrap().borrow();
            let bounds = paint.drawn.as_ref().unwrap().bounds;
            let width = columns.area.layout.track(Slot::Parent).unwrap().width;
            (
                point(bounds.left() + px(width - 1.), bounds.top() + px(100.)),
                width,
            )
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    let moved = at + point(px(60.), px(0.));
    vcx.simulate_mouse_move(moved, Some(MouseButton::Left), Modifiers::none());
    vcx.simulate_mouse_up(moved, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();
    handle
        .read_with(&vcx, |shell, _| {
            let area = &shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .area;
            let parent = area.layout.track(Slot::Parent).unwrap().width;
            assert!(
                (parent - (before + 60.)).abs() <= 1.,
                "the Parent track did not follow its hairline: {parent}"
            );
            let tracks = shell
                .launch
                .settings
                .data
                .get("preview")
                .and_then(|preview| preview.get("tracks"))
                .and_then(flea::jsondoc::Json::as_array)
                .map(<[_]>::len);
            assert_eq!(tracks, Some(2), "the tracks were not stored");
        })
        .unwrap();
}

#[test]
fn finder_sidebar_hairline_drags_through_the_width_stops_and_a_double_click_restores_192() {
    use gpui::{Modifiers, VisualTestContext};
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, host::settle);
    host::frame(&mut cx, handle);
    let at = handle
        .update(&mut cx, |shell, window, _| {
            let rail = shell.geometry(window).rail;
            assert!(rail.width > 0., "the test window has no docked rail");
            point(px(rail.x + rail.width - 1.), px(rail.y + 100.))
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    let moved = at + point(px(40.), px(0.));
    vcx.simulate_mouse_move(moved, Some(MouseButton::Left), Modifiers::none());
    vcx.simulate_mouse_up(moved, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();
    handle
        .read_with(&vcx, |shell, _| {
            assert_eq!(
                places_width(shell),
                Some(224.),
                "192 dragged 40 px is the 224 stop"
            );
            assert_eq!(shell.launch.settings.rail_width, 224.);
        })
        .unwrap();
    // The hairline now stands 32 px right, on the 224 rail's last pixel column.
    let edge = at + point(px(32.), px(0.));
    vcx.simulate_event(MouseDownEvent {
        position: edge,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    vcx.simulate_mouse_up(edge, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();
    handle
        .read_with(&vcx, |shell, _| {
            assert_eq!(
                places_width(shell),
                Some(192.),
                "a double click restores 192"
            );
        })
        .unwrap();
}
