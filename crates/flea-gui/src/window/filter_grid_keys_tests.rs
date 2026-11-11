//! Filtered Grid arrows follow visual rows through the real Shell key route.
use super::*;

fn filtered(cx: &mut TestAppContext, view: View) -> (Handle, impl FnMut() -> Option<String>) {
    filtered_to(
        cx,
        view,
        "1,3,5,7,9,11,13,15,17,19,21,23,25,27,29,31,33,35,37",
    )
}

fn filtered_to(
    cx: &mut TestAppContext,
    view: View,
    rows: &str,
) -> (Handle, impl FnMut() -> Option<String>) {
    let (handle, read) = open(cx);
    gpui::VisualTestContext::from_window(handle.into(), cx)
        .simulate_resize(gpui::size(gpui::px(800.), gpui::px(600.)));
    edit(cx, handle, |shell, _, _| {
        shell.view = view;
        shell.panes[0].as_mut().unwrap().pane.held = std::rc::Rc::new(crate::pane::Held {
            listing: 2,
            total: 48,
            start: 0,
            rows: (0..48)
                .map(|i| flea::protocol::RowFacts {
                    name: format!("file-{i:02}.txt"),
                    ..Default::default()
                })
                .collect(),
        });
    });
    frame(cx, handle);
    press(cx, handle, "/", Some("/"), false);
    press(cx, handle, "f", Some("f"), false);
    reply(
        cx,
        handle,
        &format!(
            r#"{{"t":"filtered","id":1,"query":"f","n":48,"rows":[{rows}],"ms":0.01,"listing":2}}"#
        ),
    );
    frame(cx, handle);
    (handle, read)
}

fn cursor(cx: &mut TestAppContext, handle: Handle) -> usize {
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.cursor
    })
}

fn columns(cx: &mut TestAppContext, handle: Handle) -> usize {
    edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .grid
            .as_ref()
            .unwrap()
            .state
            .borrow()
            .layout
            .unwrap()
            .columns
    })
}

#[gpui::test]
fn filtered_grid_down_commits_and_moves_one_visual_row(cx: &mut TestAppContext) {
    let (handle, _read) = filtered(cx, View::Grid);
    let stride = columns(cx, handle);
    assert!(stride > 1 && stride < 19);
    assert_eq!(cursor(cx, handle), 1);
    press(cx, handle, "down", None, false);
    assert_eq!(cursor(cx, handle), 1 + 2 * stride);
    edit(cx, handle, |shell, _, _| {
        assert!(!shell.panes[0].as_ref().unwrap().filter.owner.typing());
    });
    press(cx, handle, "up", None, false);
    assert_eq!(cursor(cx, handle), 1);
    press(cx, handle, "up", None, false);
    assert_eq!(cursor(cx, handle), 1, "the first row holds Up");
    press(cx, handle, "end", None, false);
    assert_eq!(cursor(cx, handle), 37);
    press(cx, handle, "down", None, false);
    assert_eq!(cursor(cx, handle), 37, "the last row holds Down");
    press(cx, handle, "home", None, false);
    assert_eq!(cursor(cx, handle), 1);
}

#[gpui::test]
fn filtered_grid_down_without_a_row_below_still_commits(cx: &mut TestAppContext) {
    let (handle, _read) = filtered_to(cx, View::Grid, "5,7");
    assert!(columns(cx, handle) > 2, "both matches share one visual row");
    assert_eq!(cursor(cx, handle), 5);
    edit(cx, handle, |shell, _, _| {
        assert!(shell.panes[0].as_ref().unwrap().filter.owner.typing());
    });
    press(cx, handle, "down", None, false);
    assert_eq!(cursor(cx, handle), 5, "one visual row holds Down");
    edit(cx, handle, |shell, _, _| {
        assert!(!shell.panes[0].as_ref().unwrap().filter.owner.typing());
    });
}

#[gpui::test]
fn filtered_grid_partial_last_row_keeps_the_column(cx: &mut TestAppContext) {
    let (handle, _read) = filtered(cx, View::Grid);
    press(cx, handle, "up", None, false);
    let stride = columns(cx, handle);
    assert!(stride > 1 && stride < 19);
    let remainder = 19 % stride;
    assert_ne!(remainder, 0, "fixture has a partial last row");
    let from = 19 - stride;
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 1 + 2 * from;
    });
    press(cx, handle, "down", None, false);
    assert_eq!(
        cursor(cx, handle),
        1 + 2 * from,
        "no neighbour below this column"
    );
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 1 + 2 * (19 - stride - 1);
    });
    press(cx, handle, "down", None, false);
    assert_eq!(
        cursor(cx, handle),
        37,
        "a present last-row neighbour is reached"
    );
}

#[gpui::test]
fn filtered_grid_uses_the_columns_after_resize(cx: &mut TestAppContext) {
    let (handle, _read) = filtered(cx, View::Grid);
    press(cx, handle, "up", None, false);
    let before = columns(cx, handle);
    gpui::VisualTestContext::from_window(handle.into(), cx)
        .simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    frame(cx, handle);
    let after = columns(cx, handle);
    assert_ne!(before, after);
    press(cx, handle, "down", None, false);
    assert_eq!(cursor(cx, handle), 1 + 2 * after);
}

#[gpui::test]
fn filtered_grid_page_keys_keep_the_existing_step(cx: &mut TestAppContext) {
    let (handle, _read) = filtered(cx, View::Grid);
    press(cx, handle, "up", None, false);
    let page = edit(cx, handle, |shell, _, _| {
        (shell.panes[0].as_ref().unwrap().pane.visible_rows() / 2).max(1)
    });
    press(cx, handle, "pagedown", None, false);
    assert_eq!(cursor(cx, handle), 1 + 2 * page.min(18));
    press(cx, handle, "home", None, false);
    assert_eq!(cursor(cx, handle), 1);
    press(cx, handle, "end", None, false);
    assert_eq!(cursor(cx, handle), 37);
    press(cx, handle, "pageup", None, false);
    assert_eq!(cursor(cx, handle), 1 + 2 * 18_usize.saturating_sub(page));
}

#[gpui::test]
fn filtered_list_arrows_keep_single_row_steps(cx: &mut TestAppContext) {
    let (handle, _read) = filtered(cx, View::List);
    press(cx, handle, "down", None, false);
    assert_eq!(cursor(cx, handle), 3);
    press(cx, handle, "up", None, false);
    assert_eq!(cursor(cx, handle), 1);
}
