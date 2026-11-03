//! A reread keeps the cursor tile at its drawn viewport offset after row insertion.
use super::{frame, open};
use crate::{
    backend::{Backend, Event},
    window::startup::View,
};
use gpui::{point, px, Modifiers, MouseButton, TestAppContext, VisualTestContext};

// What re-reads the folder: F5, the folder watch, or a settings relist such as the hidden-files toggle (EY-340).
#[derive(Clone, Copy, PartialEq)]
enum Trigger {
    Reload,
    Watch,
    Hidden,
}

fn reread(cx: &mut TestAppContext, trigger: Trigger) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.nav
                .after_reply(&mut host.pane, "/files", &mut host.selected);
            shell.set_view(View::Grid, window, cx);
            window.focus(&shell.focus, cx);
        })
        .unwrap();
    frame(cx, handle);
    VisualTestContext::from_window(handle.into(), cx)
        .simulate_keystrokes("down down down down down down down");
    frame(cx, handle);
    let at = handle
        .update(cx, |shell, _, _| {
            let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
            let state = grid.state.borrow();
            let columns = state.layout.unwrap().columns;
            assert!(state.scroll > 0.);
            let tile = state
                .drawn()
                .iter()
                .find(|tile| {
                    (tile.listing + 1) % columns == 0
                        && tile.cell.y > 10.
                        && tile.cell.y + tile.cell.height < f32::from(state.bounds.size.height) - 1.
                })
                .expect("a complete tile at the end of a visible tile row");
            state.bounds.origin
                + point(
                    px(tile.slot.x + tile.slot.width / 2.),
                    px(tile.slot.y + tile.slot.height / 2.),
                )
        })
        .unwrap();
    {
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        visual.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    }
    frame(cx, handle);
    let (cursor, y, name, mut rows) = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let state = host.grid.as_ref().unwrap().state.borrow();
            (
                host.pane.cursor,
                state.drawn_row(host.pane.cursor).unwrap().cell.y,
                host.pane.held.row(host.pane.cursor).unwrap().name.clone(),
                host.pane.held.rows.clone(),
            )
        })
        .unwrap();
    if trigger == Trigger::Watch {
        handle
            .update(cx, |shell, window, cx| {
                shell.receive(
                    0,
                    Backend::test_reply_line(r#"{"t":"changed","path":"/files"}"#),
                    window,
                    cx,
                );
            })
            .unwrap();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(401));
        cx.run_until_parked();
    } else if trigger == Trigger::Hidden {
        handle
            .update(cx, |shell, window, cx| {
                let patch = flea::jsondoc::parse(r#"{"hidden":true}"#).unwrap();
                let data = flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
                shell.apply_settings(data, window, cx);
            })
            .unwrap();
    } else {
        VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("f5");
    }
    assert!(handle
        .update(cx, |shell, _, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .busy())
        .unwrap());
    rows.insert(
        0,
        flea::protocol::RowFacts {
            name: "inserted.txt".into(),
            ..Default::default()
        },
    );
    let first = handle
        .update(cx, |shell, _, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .pane
                .swap
                .pending
                .as_ref()
                .unwrap()
                .first
        })
        .unwrap();
    while rows.len() < first {
        rows.push(flea::protocol::RowFacts {
            name: format!("file-{}.txt", rows.len() - 1),
            ..Default::default()
        });
    }
    rows.truncate(first);
    handle
        .update(cx, |shell, window, cx| {
            for reply in [
                flea::protocol::Reply::Listed {
                    path: "/files".into(),
                    total: 1001,
                },
                flea::protocol::Reply::Rows {
                    start: 0,
                    rows,
                    listing: 3,
                },
            ] {
                shell.receive(0, Event::Reply(reply), window, cx);
            }
        })
        .unwrap();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(401));
    cx.run_until_parked();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, cursor + 1);
            assert_eq!(host.pane.held.row(host.pane.cursor).unwrap().name, name);
            let state = host.grid.as_ref().unwrap().state.borrow();
            let tile = state
                .drawn_row(host.pane.cursor)
                .expect("the anchored cursor tile is drawn");
            assert!(
                (tile.cell.y - y).abs() < 0.01,
                "tile moved from {y} to {}",
                tile.cell.y
            );
        })
        .unwrap();
    handle
        .update(cx, |shell, _, _| shell.stop_for_app_quit())
        .unwrap();
}

#[gpui::test]
fn pb_path_grid_reload_preserves_the_cursor_tile_offset(cx: &mut TestAppContext) {
    reread(cx, Trigger::Reload);
}

#[gpui::test]
fn pb_path_grid_watch_preserves_the_cursor_tile_offset(cx: &mut TestAppContext) {
    reread(cx, Trigger::Watch);
}

#[gpui::test]
fn ey340_grid_hidden_toggle_preserves_the_cursor_tile_offset(cx: &mut TestAppContext) {
    reread(cx, Trigger::Hidden);
}
