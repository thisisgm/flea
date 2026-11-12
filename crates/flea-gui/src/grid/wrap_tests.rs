//! Row crossing follows the same key, selection and reveal path as movement inside a row.
use super::join_tests::{frame, key, open};
use crate::{
    grid::Shown,
    state::Shell,
    window::startup::{Settings, View},
};
use gpui::{TestAppContext, WindowHandle};

pub(super) fn configured(
    cx: &mut TestAppContext,
    document: &str,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<(usize, usize)>) {
    let (handle, requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.launch.settings = Settings::from_json(flea::uistate::from_file(document));
            shell.set_view(View::Grid, window, cx);
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            assert!(
                !shell.panes[0].as_ref().unwrap().pane.busy(),
                "navigation fixture must be idle"
            );
        })
        .unwrap();
    (handle, requests)
}

#[gpui::test]
fn issue_434_shift_horizontal_crosses_rows_and_keeps_the_selection_anchor(cx: &mut TestAppContext) {
    let (handle, _requests) = configured(cx, r#"{"gridWrapRows":true}"#);
    let columns = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let columns = host
                .grid
                .as_ref()
                .unwrap()
                .view
                .layout(Shown::All(host.pane.held.total))
                .columns;
            host.pane.cursor = columns - 2;
            columns
        })
        .unwrap();
    assert!(columns > 1);
    key(cx, handle, "shift-right");
    key(cx, handle, "shift-right");
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, columns);
            assert_eq!(host.selected, vec![columns - 2, columns - 1, columns]);
        })
        .unwrap();
    key(cx, handle, "shift-left");
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, columns - 1);
            assert_eq!(host.selected, vec![columns - 2, columns - 1]);
        })
        .unwrap();
    handle
        .update(cx, |shell, window, cx| {
            shell.change_settings(
                flea::jsondoc::parse(r#"{"gridWrapRows":false}"#).unwrap(),
                window,
                cx,
            );
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.cursor = columns - 2;
            host.input.selection.clear(&mut host.selected);
        })
        .unwrap();
    key(cx, handle, "shift-right");
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                host.pane.cursor,
                columns - 1,
                "off extends within the current row"
            );
            assert_eq!(host.selected, vec![columns - 2, columns - 1]);
        })
        .unwrap();
}

#[gpui::test]
fn issue_434_folder_end_crossing_requires_both_settings(cx: &mut TestAppContext) {
    let (handle, _requests) = configured(cx, r#"{"gridWrapRows":true,"wrapAtEnds":true}"#);
    key(cx, handle, "left");
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, host.pane.held.total - 1);
        })
        .unwrap();
    key(cx, handle, "right");
    handle
        .update(cx, |shell, _, _| {
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 0);
        })
        .unwrap();
    handle
        .update(cx, |shell, window, cx| {
            shell.change_settings(
                flea::jsondoc::parse(r#"{"gridWrapRows":false}"#).unwrap(),
                window,
                cx,
            );
        })
        .unwrap();
    key(cx, handle, "left");
    handle
        .update(cx, |shell, _, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            assert_eq!(
                host.pane.cursor, 0,
                "list wrapping alone does not cross Grid rows"
            );
            host.pane.cursor = host.pane.held.total - 1;
            assert!(shell.grid_key("cursorRight", cx));
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, host.pane.held.total - 1);
        })
        .unwrap();
}

#[gpui::test]
fn shift_at_a_folder_end_stops_as_the_list_does(cx: &mut TestAppContext) {
    let (handle, _requests) = configured(cx, r#"{"gridWrapRows":true,"wrapAtEnds":true}"#);
    key(cx, handle, "shift-left");
    let last = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let last = host.pane.held.total - 1;
            assert_eq!(
                host.pane.cursor, 0,
                "Shift never wraps past the folder start"
            );
            assert!(!host.selected.contains(&last));
            host.pane.cursor = last;
            host.input.selection.clear(&mut host.selected);
            last
        })
        .unwrap();
    key(cx, handle, "shift-right");
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                host.pane.cursor, last,
                "Shift never wraps past the folder end"
            );
            assert!(!host.selected.contains(&0));
        })
        .unwrap();
}
