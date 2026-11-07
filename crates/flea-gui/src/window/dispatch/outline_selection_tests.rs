//! After a tree folder opens in place, a lone selection keeps following the cursor.
use super::*;

#[gpui::test]
fn tree_outline_lone_selection_keeps_following_the_cursor_after_open(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("A", true), row("end.txt", false)],
        1,
    );
    // The --select launch landing: A is a lone mark the cursor carries.
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.cursor = 0;
        host.input.selection.only(&mut host.selected, 0);
    });
    key(cx, "ctrl-right");
    let mut folder = row("A", true);
    folder.open = true;
    let mut child = row("A/child.txt", false);
    child.depth = 1;
    land(
        &shell,
        cx,
        "/files",
        vec![folder, child, row("end.txt", false)],
        2,
    );
    key(cx, "down");
    let (cursor, selected) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.pane.cursor, host.selected.clone())
    });
    assert_eq!(
        (cursor, selected),
        (1, vec![1]),
        "the lone selection follows the cursor into the open folder"
    );
}
