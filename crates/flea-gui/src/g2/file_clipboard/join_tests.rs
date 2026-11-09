//! The production Shell join: real key dispatch, backend replies and op1's collision host, with no test subscriber on any emitted action.
use crate::{
    backend::{Backend, Event as BackendEvent},
    row::marks::ClipMark,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use gpui::{AppContext, Context, TestAppContext, VisualTestContext, Window, WindowHandle};

#[path = "m9_g2rows_23_ipc_tests.rs"]
mod m9_g2rows_23_ipc_tests;

mod pb_clipboard_tests;

type Handle = WindowHandle<Root>;

fn open(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Vec<String>) {
    let (backend, mut lines) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    edit(cx, handle, |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        assert!(pane.begin("/files".into(), Default::default(), pane.now()));
        for reply in [
            flea::protocol::Reply::Listed {
                path: "/files".into(),
                total: 3,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                listing: 3,
                rows: ["a.txt", "b.txt", "c.txt"]
                    .iter()
                    .map(|name| flea::protocol::RowFacts {
                        name: (*name).into(),
                        ..Default::default()
                    })
                    .collect(),
            },
        ] {
            shell.receive(0, BackendEvent::Reply(reply), window, cx);
        }
        shell.panes[0].as_mut().unwrap().pane.cursor = 0;
    });
    frame(cx, handle);
    let drain = move || std::iter::from_fn(&mut lines).collect::<Vec<_>>();
    (handle, drain)
}

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| change(shell, window, cx))
        })
        .unwrap()
}

fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn key(cx: &mut TestAppContext, handle: Handle, value: &str) {
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(value);
    frame(cx, handle);
}

fn reply(cx: &mut TestAppContext, handle: Handle, line: &str) {
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(line), window, cx)
    });
    frame(cx, handle);
}

fn list_mark(cx: &mut TestAppContext, handle: Handle, name: &str) -> Option<ClipMark> {
    edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .list
            .borrow()
            .view
            .clip
            .mark_for(name)
    })
}

// Cut a.txt by key, land it on the clipboard, then copy b.txt while the cut's paste is still asking.
fn cut_then_paste(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Vec<String>) {
    let (handle, mut lines) = open(cx);
    lines();
    key(cx, handle, "ctrl-x");
    assert_eq!(
        lines(),
        vec![
            r#"{"c":"clipWatch"}"#.to_string(),
            r#"{"c":"paths","rows":[0]}"#.to_string()
        ],
        "M9_CUT_ASKS_ITS_ROWS"
    );
    reply(cx, handle, r#"{"t":"paths","paths":["/files/a.txt"]}"#);
    assert_eq!(
        lines(),
        vec![r#"{"c":"clipSet","op":"cut","paths":["/files/a.txt"]}"#.to_string()],
        "M9_CUT_CLAIMS_ONCE"
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"set","ok":true,"token":"T1"}"#,
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"cut","paths":["/files/a.txt"],"token":"T1","skipped":0}"#,
    );
    assert_eq!(
        list_mark(cx, handle, "a.txt"),
        Some(ClipMark::Cut),
        "M9_LIST_DRAWS_THE_CUT"
    );
    assert_eq!(list_mark(cx, handle, "b.txt"), None);
    key(cx, handle, "ctrl-v");
    let asked = lines();
    assert!(
        asked
            == vec![
                r#"{"c":"collisions","id":1,"dest":"/files","paths":["/files/a.txt"]}"#.to_string()
            ],
        "M9_PASTE_REACHES_OP1: {asked:?}"
    );
    assert!(
        edit(cx, handle, |shell, _, _| shell.clipboard.captured[0]
            .is_some()),
        "M9_PASTE_CAPTURES_ITS_ENTRY"
    );
    (handle, lines)
}

#[gpui::test]
fn m9_integ_paste_completion_clears_only_captured_cut(cx: &mut TestAppContext) {
    let (handle, mut lines) = cut_then_paste(cx);
    // A newer copy replaces the selection while the cut's paste is still asking.
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 1
    });
    key(cx, handle, "ctrl-c");
    assert_eq!(lines(), vec![r#"{"c":"paths","rows":[1]}"#.to_string()]);
    reply(cx, handle, r#"{"t":"paths","paths":["/files/b.txt"]}"#);
    assert_eq!(
        lines(),
        vec![r#"{"c":"clipSet","op":"copy","paths":["/files/b.txt"]}"#.to_string()]
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"set","ok":true,"token":"T2"}"#,
    );
    // A stale collision answer for another question spends nothing.
    reply(
        cx,
        handle,
        r#"{"t":"collisions","id":99,"total":0,"names":[],"mergeable":false}"#,
    );
    assert!(lines().is_empty(), "M9_STALE_ANSWER_SENDS_NOTHING");
    assert!(edit(cx, handle, |shell, _, _| shell.clipboard.captured[0]
        .is_some()));
    reply(
        cx,
        handle,
        r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
    );
    let sent = lines();
    assert_eq!(sent.len(), 2, "M9_TRANSFER_THEN_CLEAR: {sent:?}");
    assert!(
        sent[0].contains(r#""c":"transfer""#)
            && sent[0].contains(r#""op":"move""#)
            && sent[0].contains("/files/a.txt")
    );
    assert_eq!(
        sent[1], r#"{"c":"clipClear","token":"T1"}"#,
        "M9_CLEARS_ONLY_THE_CAPTURED_CUT"
    );
    assert!(
        edit(cx, handle, |shell, _, _| shell.clipboard.captured[0]
            .is_none()),
        "M9_SPENT_RELEASES_THE_CAPTURE"
    );
    let current = edit(cx, handle, |shell, _, _| {
        shell.clipboard.board.current().clone()
    });
    assert_eq!(
        (current.paths, current.moving, current.token.as_str()),
        (vec!["/files/b.txt".to_string()], false, "T2")
    );
    assert_eq!(
        list_mark(cx, handle, "b.txt"),
        Some(ClipMark::Copy),
        "M9_NEWER_COPY_KEEPS_ITS_MARK"
    );
    assert_eq!(list_mark(cx, handle, "a.txt"), None);
}

#[gpui::test]
fn a_spent_cut_with_nothing_newer_empties_the_clipboard_and_its_marks(cx: &mut TestAppContext) {
    let (handle, mut lines) = cut_then_paste(cx);
    reply(
        cx,
        handle,
        r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
    );
    let sent = lines();
    assert_eq!(
        sent.last().map(String::as_str),
        Some(r#"{"c":"clipClear","token":"T1"}"#)
    );
    assert!(edit(cx, handle, |shell, _, _| shell
        .clipboard
        .board
        .current()
        .paths
        .is_empty()));
    assert_eq!(
        list_mark(cx, handle, "a.txt"),
        None,
        "M9_SPENT_CUT_DROPS_ITS_MARK"
    );
}

#[gpui::test]
fn a_cancelled_collision_releases_the_capture_and_keeps_the_cut(cx: &mut TestAppContext) {
    let (handle, mut lines) = cut_then_paste(cx);
    reply(
        cx,
        handle,
        r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"a.txt","d":false,"i":"text-plain"}],"mergeable":false}"#,
    );
    assert!(
        edit(cx, handle, |shell, _, _| shell.clipboard.captured[0]
            .is_some()),
        "the open card still holds the capture"
    );
    key(cx, handle, "escape");
    let sent = lines();
    assert!(
        sent.iter().all(|line| !line.contains("clipClear")),
        "M9_CANCEL_NEVER_CLEARS: {sent:?}"
    );
    assert!(
        edit(cx, handle, |shell, _, _| shell.clipboard.captured[0]
            .is_none()),
        "M9_CANCEL_RELEASES_THE_CAPTURE"
    );
    assert_eq!(list_mark(cx, handle, "a.txt"), Some(ClipMark::Cut));
}

#[gpui::test]
fn a_paths_reply_nobody_asked_the_clipboard_for_is_left_to_its_owner(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    lines();
    reply(cx, handle, r#"{"t":"paths","paths":["/files/a.txt"]}"#);
    assert!(lines().is_empty(), "M9_UNASKED_PATHS_CLAIM_NOTHING");
    assert!(edit(cx, handle, |shell, _, _| shell
        .clipboard
        .board
        .current()
        .paths
        .is_empty()));
}

#[gpui::test]
fn columns_and_grid_read_the_same_marks(cx: &mut TestAppContext) {
    let (handle, _lines) = cut_then_paste(cx);
    edit(cx, handle, |shell, window, cx| {
        let marks = shell
            .clipboard
            .column_marks()
            .expect("a held cut lends Columns its marks");
        assert_eq!(
            marks.mark("/files", "a.txt"),
            crate::columns::paint::Clip::Cut
        );
        assert_eq!(
            marks.mark("/files", "b.txt"),
            crate::columns::paint::Clip::None
        );
        shell.set_view(crate::window::startup::View::Grid, window, cx);
    });
    // The frame runs sync_clip_marks, which hands the Grid host its decor; the test reads that decor, never one it built.
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let grid = shell.panes[0]
            .as_ref()
            .unwrap()
            .grid
            .as_ref()
            .expect("the pane draws a Grid");
        let row = flea::protocol::RowFacts {
            name: "a.txt".into(),
            ..Default::default()
        };
        let clip = grid
            .decor
            .clip_row(0, Some(&row))
            .expect("the Grid tile is marked");
        assert!(clip.cut);
        assert_eq!(clip.key.as_ref(), "flea-mark:scissors");
    });
}

#[gpui::test]
fn a_rows_menu_cut_publishes_the_validated_snapshot_once(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    lines();
    key(cx, handle, "m");
    let id = lines()
        .iter()
        .find(|line| line.contains(r#""op":"snapshot""#))
        .and_then(|line| flea::json::field_usize(line, "id"))
        .expect("M9_MENU_ASKS_ITS_SNAPSHOT");
    for operation in ["snapshot", "applications"] {
        reply(
            cx,
            handle,
            &format!(
                r#"{{"t":"menuaction","id":{id},"op":"{operation}","ok":true,"kinds":[32768],"applications":[]}}"#
            ),
        );
    }
    lines();
    // The row pick the card makes: its action becomes the pending choice and activates.
    edit(cx, handle, |shell, window, cx| {
        assert!(
            shell
                .menu_entries()
                .iter()
                .any(|entry| entry.action == "cut"),
            "M9_MENU_LISTS_CUT"
        );
        shell.menu.pending = Some(("cut".into(), String::new()));
        shell.activate_menu(window, cx);
    });
    frame(cx, handle);
    let validate = lines();
    assert!(
        validate
            .iter()
            .any(|line| line.contains(r#""op":"validate""#) && line.contains(r#""action":"cut""#)),
        "M9_MENU_CUT_VALIDATES: {validate:?}"
    );
    reply(
        cx,
        handle,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"validate","ok":true,"action":"cut","paths":["/files/a.txt"]}}"#
        ),
    );
    let sent = lines();
    assert_menu_cut_sent(&sent);
    let status = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().status.to_string()
    });
    assert_eq!(status, "Cut 1 item");
}

fn assert_menu_cut_sent(sent: &[String]) {
    let published: Vec<_> = sent
        .iter()
        .filter(|line| line.contains(r#""c":"clipSet""#))
        .map(String::as_str)
        .collect();
    assert_eq!(
        published,
        [r#"{"c":"clipSet","op":"cut","paths":["/files/a.txt"]}"#],
        "MENU_CUT_PUBLISHES_ONCE: {sent:?}"
    );
    assert!(
        !sent.iter().any(|line| line.contains(r#""c":"paths""#)),
        "M9_MENU_CUT_CLAIMS_THE_SNAPSHOT: {sent:?}"
    );
}

#[test]
#[should_panic(expected = "MENU_CUT_PUBLISHES_ONCE")]
fn a_duplicate_clipboard_publication_fails_the_menu_trace_check() {
    let expected = r#"{"c":"clipSet","op":"cut","paths":["/files/a.txt"]}"#.to_string();
    assert_menu_cut_sent(&[expected.clone(), expected]);
}

// EY-346: after a one-file copy lands, Ctrl+A then y asks for every selected row and publishes all three.
#[gpui::test]
fn ey346_select_all_then_copy_publishes_every_selected_path(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    lines();
    key(cx, handle, "y");
    assert_eq!(
        lines().last().map(String::as_str),
        Some(r#"{"c":"paths","rows":[0]}"#)
    );
    reply(cx, handle, r#"{"t":"paths","paths":["/files/a.txt"]}"#);
    assert_eq!(
        lines(),
        vec![r#"{"c":"clipSet","op":"copy","paths":["/files/a.txt"]}"#.to_string()]
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"set","ok":true,"token":"T1"}"#,
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"copy","paths":["/files/a.txt"],"token":"T1","skipped":0}"#,
    );
    key(cx, handle, "ctrl-a");
    key(cx, handle, "y");
    assert_eq!(
        lines(),
        vec![r#"{"c":"paths","rows":[0,1,2]}"#.to_string()],
        "EY346_COPY_ASKS_EVERY_SELECTED_ROW"
    );
    reply(
        cx,
        handle,
        r#"{"t":"paths","paths":["/files/a.txt","/files/b.txt","/files/c.txt"]}"#,
    );
    assert_eq!(
        lines(),
        vec![
            r#"{"c":"clipSet","op":"copy","paths":["/files/a.txt","/files/b.txt","/files/c.txt"]}"#
                .to_string()
        ],
        "EY346_COPY_PUBLISHES_ALL_THREE"
    );
}
