//! Regression checks for explicit Grid cursor reveal actions.
use super::{grid, notch, open, scroll_and_max};
use crate::window::startup::View;
use gpui::TestAppContext;

// EY-184: Home on the first item scrolls a wheeled Grid back to the top, as v0.3.12 ui.sh overlays expects.
#[gpui::test]
fn nav_actions_ey184_home_after_wheel_scrolls_the_grid_to_the_top(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    crate::grid::join_tests::frame(cx, handle);
    for _ in 0..3 {
        notch(cx, handle, -3.);
    }
    crate::grid::join_tests::frame(cx, handle);
    assert_eq!(scroll_and_max(cx, handle).0, 864.);
    let cursor = |cx: &mut TestAppContext| {
        handle
            .update(cx, |shell, _, _| {
                shell.panes[0].as_ref().unwrap().pane.cursor
            })
            .unwrap()
    };
    assert_eq!(
        cursor(cx),
        0,
        "the wheel leaves the cursor on the first item"
    );
    crate::grid::join_tests::key(cx, handle, "home");
    crate::grid::join_tests::frame(cx, handle);
    assert_eq!(cursor(cx), 0);
    assert_eq!(
        scroll_and_max(cx, handle).0,
        0.,
        "Home brings the first row back"
    );
    notch(cx, handle, -3.);
    crate::grid::join_tests::frame(cx, handle);
    handle
        .update(cx, |shell, _, cx| {
            assert!(shell.grid_key("cursorLast", cx));
            assert!(shell.grid_key("cursorFirst", cx));
        })
        .unwrap();
    crate::grid::join_tests::frame(cx, handle);
    assert_eq!(scroll_and_max(cx, handle).0, 0.);
}

#[gpui::test]
fn nav_actions_ey184_home_includes_remaining_top_padding(cx: &mut TestAppContext) {
    let handle = grid(cx);
    handle
        .update(cx, |shell, _, cx| assert!(shell.grid_scroll(1., cx)))
        .unwrap();
    crate::grid::join_tests::frame(cx, handle);
    assert_eq!(scroll_and_max(cx, handle).0, 1.);
    crate::grid::join_tests::key(cx, handle, "home");
    assert_eq!(scroll_and_max(cx, handle).0, 0.);
}

#[gpui::test]
fn nav_actions_end_after_wheel_reveals_an_unchanged_last_cursor(cx: &mut TestAppContext) {
    let (backend, _requests) = crate::backend::Backend::test_channel();
    let handle = cx.add_window(|_, cx| {
        let mut shell = crate::grid::join_tests::named_shell(backend, cx);
        let held = &mut shell.panes[0].as_mut().unwrap().pane.held;
        let held = std::rc::Rc::get_mut(held).unwrap();
        held.total = held.rows.len();
        shell
    });
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    crate::grid::join_tests::frame(cx, handle);
    crate::grid::join_tests::key(cx, handle, "end");
    let landed = scroll_and_max(cx, handle).0;
    handle
        .update(cx, |shell, _, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, host.pane.held.total - 1);
            let grid = host.grid.as_ref().unwrap();
            let layout = grid
                .view
                .layout(crate::grid::Shown::All(host.pane.held.total));
            assert!(shell.grid_scroll(-layout.height - layout.cell_height, cx));
        })
        .unwrap();
    crate::grid::join_tests::frame(cx, handle);
    assert!(scroll_and_max(cx, handle).0 < landed);
    crate::grid::join_tests::key(cx, handle, "end");
    assert_eq!(scroll_and_max(cx, handle).0, landed);
}
