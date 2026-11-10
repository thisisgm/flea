//! Issue 434: horizontal row wrapping is a persisted, opt-in Grid behavior.
use super::join_tests::{frame, open};
use crate::{grid::Shown, window::startup::View};
use gpui::TestAppContext;

#[gpui::test]
fn issue_434_grid_horizontal_wrap_obeys_setting_filter_and_listing_ends(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    handle.update(cx, |shell, window, cx| shell.set_view(View::Grid, window, cx)).unwrap();
    frame(cx, handle);
    let columns = handle.update(cx, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        host.grid.as_ref().unwrap().view.layout(Shown::All(host.pane.held.total)).columns
    }).unwrap();
    assert!(columns > 1);
    let cursor = |cx: &mut TestAppContext| handle.update(cx, |shell, _, _| shell.panes[0].as_ref().unwrap().pane.cursor).unwrap();

    handle.update(cx, |shell, _, cx| {
        shell.panes[0].as_mut().unwrap().pane.cursor = columns - 1;
        assert!(shell.grid_key("cursorRight", cx));
        assert!(shell.grid_key("extendRight", cx));
    }).unwrap();
    assert_eq!(cursor(cx), columns - 1, "the default keeps row edges clamped");
    assert!(handle.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().selected.is_empty()).unwrap(),
        "a blocked extension does not start a range");
    handle.update(cx, |shell, window, cx| {
        shell.change_settings(flea::jsondoc::parse(r#"{"gridWrapRows":true}"#).unwrap(), window, cx);
        shell.panes[0].as_mut().unwrap().pane.cursor = columns - 1;
        assert!(shell.grid_key("cursorRight", cx));
    }).unwrap();
    assert_eq!(cursor(cx), columns, "enabled wrapping enters the next row");
    handle.update(cx, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.input.selection.toggle(&mut host.selected, 0);
        host.pane.cursor = columns - 1;
    }).unwrap();
    handle.update(cx, |shell, _, cx| {
        assert!(shell.grid_key("extendRight", cx));
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0, columns - 1, columns]);
        assert!(shell.grid_key("extendRight", cx));
        assert!(shell.grid_key("extendLeft", cx));
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0, columns - 1, columns]);
        assert!(shell.grid_key("extendLeft", cx));
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0, columns - 1]);
    }).unwrap();
    handle.update(cx, |shell, _, cx| assert!(shell.grid_key("cursorRight", cx))).unwrap();
    assert_eq!(cursor(cx), columns, "ordinary movement follows the range gesture");
    handle.update(cx, |shell, _, cx| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 0;
        assert!(shell.grid_key("cursorLeft", cx));
    }).unwrap();
    assert_eq!(cursor(cx), 0, "the beginning remains a boundary");
    let last = handle.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.held.total - 1).unwrap();
    handle.update(cx, |shell, _, cx| {
        shell.panes[0].as_mut().unwrap().pane.cursor = last;
        assert!(shell.grid_key("cursorRight", cx));
    }).unwrap();
    assert_eq!(cursor(cx), last, "the incomplete final row has no phantom cell");
    handle.update(cx, |shell, _, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        let shown: Vec<usize> = (0..=columns).map(|view| view * 2).collect();
        host.input.shown = Some(shown.clone().into());
        host.pane.cursor = shown[columns - 1];
        assert!(shell.grid_key("cursorRight", cx));
    }).unwrap();
    assert_eq!(cursor(cx), columns * 2, "filtered visible order determines the wrapped neighbor");
}
