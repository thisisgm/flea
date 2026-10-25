//! The keyboard menu in Columns opens over the active column's cursor row, as ColumnsArea.qml anchors it.
use crate::{columns::geometry::Slot, window::columns_tests::host};
use gpui::VisualTestContext;

#[test]
fn the_menu_key_in_columns_anchors_on_the_active_columns_cursor_row() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(
            shell,
            "/srv/a/b",
            &[("c", true), ("sibling", true)],
            false,
            window,
            cx,
        );
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false)],
            false,
            window,
            cx,
        );
        shell.panes[0].as_mut().unwrap().pane.cursor = 1;
        window.focus(&shell.focus, cx);
    });
    host::frame(&mut cx, window);
    let (active, parent) = window
        .update(&mut cx, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let origin = |slot| {
                let paint = columns.painters.get(slot).unwrap().borrow();
                let bounds = paint.drawn.as_ref().unwrap().bounds;
                (f32::from(bounds.origin.x), f32::from(bounds.origin.y))
            };
            (origin(Slot::Active), origin(Slot::Parent))
        })
        .unwrap();
    assert!(
        active.0 > parent.0,
        "the active column is right of the parent"
    );
    VisualTestContext::from_window(window.into(), &cx).simulate_keystrokes("m");
    let at = window
        .update(&mut cx, |shell, _, _| shell.menu.presentation.at)
        .unwrap()
        .expect("the menu key opens the menu");
    let row = crate::list::pane::ROW_HEIGHT;
    assert_eq!(
        at,
        (active.0 + 14., active.1 + row + row / 2.),
        "over d02 in the active column"
    );
}
