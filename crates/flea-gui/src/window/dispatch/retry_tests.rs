//! A transfer that left sources behind marks them again in their folder and says so on the status line.
use super::test_shell::{land, open, row, set_cursor};
use crate::{
    backend::{Event as BackendEvent, Located},
    state::Shell,
};
use gpui::{Entity, TestAppContext, VisualTestContext};

#[path = "transfer_keep_tests.rs"]
mod transfer_keep;

const DONE: &str = r#"{"t":"transferdone","id":7,"ok":1,"failed":2,"skipped":0,"cancelled":false,"retryPaths":["/files/beta","/files/notes.txt"],"durable":false,"note":""}"#;

fn home() -> Vec<flea::protocol::RowFacts> {
    vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ]
}

fn receive(shell: &Entity<Shell>, cx: &mut VisualTestContext, event: BackendEvent) {
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.receive(0, event, window, cx)));
    cx.run_until_parked();
}

// The id of the locate the retry asked, once it is out.
fn asked(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<u64> {
    shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .retry
            .as_ref()
            .and_then(|retry| retry.locate)
    })
}

fn answer(id: u64) -> BackendEvent {
    BackendEvent::Located(Located {
        directory: "/files".into(),
        path: String::new(),
        index: -1,
        id: Some(id),
        transfer: 0,
        ok: true,
        matches: vec![("/files/beta".into(), 1), ("/files/notes.txt".into(), 2)],
    })
}

fn marks(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (Vec<usize>, usize, String) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.selected.clone(), host.pane.cursor, host.retry_text())
    })
}

#[gpui::test]
fn failed_sources_are_marked_again_and_the_line_goes_when_the_cursor_moves(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    receive(&shell, cx, BackendEvent::Operations(DONE.into()));
    // The transfer's refresh listing lands first; the retry asks once the folder is idle (ui/PaneWire.qml locateRetry).
    land(&shell, cx, "/files", home(), 2);
    let id = asked(&shell, cx).expect("the idle folder asks where the failed sources are");
    let line = shell.read_with(cx, |shell, _| {
        flea::ui_logic_file_ops::lines::retry_selection_line(
            &shell.locale.catalog,
            &["/files/beta", "/files/notes.txt"],
        )
    });
    assert!(!line.is_empty());
    receive(&shell, cx, answer(id + 1));
    assert_eq!(
        marks(&shell, cx).0,
        Vec::<usize>::new(),
        "another asker's reply marks nothing"
    );
    receive(&shell, cx, answer(id));
    assert_eq!(marks(&shell, cx), (vec![1, 2], 1, line));
    set_cursor(&shell, cx, 0);
    assert_eq!(
        marks(&shell, cx).2,
        "",
        "the line goes with the selection it was written for"
    );
}

#[gpui::test]
fn a_re_read_before_the_answer_marks_nothing(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    receive(&shell, cx, BackendEvent::Operations(DONE.into()));
    // The transfer's refresh listing lands first; the retry asks once the folder is idle (ui/PaneWire.qml locateRetry).
    land(&shell, cx, "/files", home(), 2);
    let id = asked(&shell, cx).expect("the retry asked");
    land(&shell, cx, "/files", home(), 3);
    receive(&shell, cx, answer(id));
    assert_eq!(marks(&shell, cx), (Vec::new(), 0, String::new()));
    let next = asked(&shell, cx).expect("the same folder still owes its retry selection");
    assert_ne!(next, id, "stale locate indices must be asked again");
    receive(&shell, cx, answer(id));
    assert!(marks(&shell, cx).0.is_empty());
    receive(&shell, cx, answer(next));
    assert_eq!(marks(&shell, cx).0, [1, 2]);
}
