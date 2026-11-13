//! An undo receipt must refresh even when the filesystem sends no watch notice.
use super::*;

fn ready(cx: &mut TestAppContext) -> Run<'_> {
    let (shell, cx) = launched(cx, "list");
    let (backend, read) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    Run {
        shell,
        cx,
        read: Box::new(read),
        listing: 1,
        sent: Vec::new(),
    }
}

fn lists(run: &Run<'_>) -> usize {
    run.sent
        .iter()
        .filter(|line| flea::json::field_str(line, "c").as_deref() == Some("list"))
        .count()
}

#[gpui::test]
fn undone_trash_lists_the_restored_row_without_a_watch_notice(cx: &mut TestAppContext) {
    story(
        cx,
        "unwatched undo",
        &[
            Step::Delete,
            Step::Trashed,
            Step::Land(false),
            Step::Wait(1000),
            Step::Undo,
            Step::Undone,
            Step::Land(true),
        ],
    );
}

#[gpui::test]
fn an_undo_receipt_during_the_trash_reread_lists_again_without_a_watch_notice(
    cx: &mut TestAppContext,
) {
    story(
        cx,
        "unwatched undo during trash reread",
        &[
            Step::Delete,
            Step::Trashed,
            Step::Undo,
            Step::Undone,
            Step::Land(false),
            Step::Land(true),
        ],
    );
}

#[gpui::test]
fn redone_trash_removes_the_row_without_a_watch_notice(cx: &mut TestAppContext) {
    let mut run = ready(cx);
    run.cx.simulate_keystrokes("ctrl-shift-z");
    run.frame();
    assert!(run.sent.iter().any(|line| line == r#"{"c":"redo"}"#));
    run.receive(Backend::test_reply_line(
        r#"{"t":"redone","op":"trash","ok":true}"#,
    ));
    run.step(Step::Land(false));
    run.quiesce();
    assert_eq!(lists(&run), 1, "redo owes one listing with no watch notice");
    run.shell.read_with(run.cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.held.total, 3);
        assert_eq!(host.list.borrow().drawn_rows.total, 3);
        assert!(host
            .pane
            .held
            .rows
            .iter()
            .all(|row| row.name != "notes.txt"));
    });
    run.shell
        .update(run.cx, |shell, _| shell.stop_for_app_quit());
}

fn position(run: &Run<'_>) -> (usize, Vec<usize>, f32, Vec<String>) {
    run.shell.read_with(run.cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (
            host.pane.cursor,
            host.selected.clone(),
            host.pane.scroll_y,
            host.list
                .borrow()
                .drawn_rows
                .rows
                .iter()
                .map(|row| row.name.clone())
                .collect(),
        )
    })
}

#[gpui::test]
fn a_watched_undo_keeps_its_rows_cursor_and_scroll_during_the_following_reread(
    cx: &mut TestAppContext,
) {
    let mut run = ready(cx);
    for step in [
        Step::Delete,
        Step::Trashed,
        Step::Land(false),
        Step::Wait(1000),
        Step::Undo,
        Step::Undone,
        Step::Changed,
        Step::Land(true),
    ] {
        run.step(step);
    }
    run.agree("undo receipt landed");
    let restored = position(&run);
    assert_eq!(restored.0, 2, "the restored row takes its original index");
    assert_eq!(restored.3[restored.0], "notes.txt");
    assert_eq!(lists(&run), 2, "Trash and undo each owe a listing");
    run.step(Step::Wait(500));
    assert_eq!(lists(&run), 3, "the watch keeps its own later-write check");
    assert_eq!(
        position(&run),
        restored,
        "an in-flight reread keeps the frame"
    );
    run.step(Step::Land(true));
    assert_eq!(
        position(&run),
        restored,
        "the watch swap keeps the restored row"
    );
    run.quiesce();
    assert_eq!(lists(&run), 3, "no repeating watch or polling");
    run.agree("watched undo settled");
    run.shell
        .update(run.cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn an_undo_refresh_covers_the_watch_notice_that_preceded_its_request(cx: &mut TestAppContext) {
    let mut run = ready(cx);
    for step in [
        Step::Delete,
        Step::Trashed,
        Step::Land(false),
        Step::Wait(1000),
        Step::Undo,
        Step::Changed,
        Step::Undone,
        Step::Land(true),
    ] {
        run.step(step);
    }
    let restored = position(&run);
    run.quiesce();
    assert_eq!(lists(&run), 2, "the undo read covers the earlier notice");
    assert_eq!(position(&run), restored);
    run.agree("the earlier undo notice is covered");
    run.shell
        .update(run.cx, |shell, _| shell.stop_for_app_quit());
}

fn land_rows(run: &mut Run<'_>, rows: Vec<RowFacts>) {
    run.listing += 1;
    run.receive(BackendEvent::Reply(Reply::Listed {
        path: "/files".into(),
        total: rows.len(),
    }));
    run.receive(BackendEvent::Reply(Reply::Rows {
        start: 0,
        rows,
        listing: run.listing,
    }));
}

#[gpui::test]
fn undo_and_the_watch_swap_keep_a_scrolled_viewport(cx: &mut TestAppContext) {
    let mut run = ready(cx);
    run.cx.update(|window, cx| {
        run.shell.update(cx, |shell, cx| {
            let _ = shell.step(0, crate::window::navigate::Step::Refresh(None), window, cx);
        })
    });
    let rows: Vec<_> = (0..100)
        .map(|i| super::super::super::test_shell::row(&format!("file-{i:03}.txt"), false))
        .collect();
    land_rows(&mut run, rows.clone());
    run.cx.simulate_keystrokes("shift-g");
    run.step(Step::Wait(500));
    assert!(
        position(&run).2 > 0.,
        "the viewport must actually be scrolled"
    );
    run.step(Step::Delete);
    run.step(Step::Trashed);
    land_rows(&mut run, rows[..99].to_vec());
    run.step(Step::Wait(1000));
    let scroll = position(&run).2;
    run.step(Step::Undo);
    run.step(Step::Undone);
    run.step(Step::Changed);
    land_rows(&mut run, rows.clone());
    let restored = position(&run);
    assert_eq!(restored.2, scroll, "undo keeps the viewport");
    run.step(Step::Wait(500));
    assert_eq!(
        position(&run),
        restored,
        "the watch request keeps the frame"
    );
    land_rows(&mut run, rows);
    assert_eq!(
        position(&run),
        restored,
        "the watch landing keeps the frame"
    );
    run.shell
        .update(run.cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn an_unsuccessful_undo_or_redo_does_not_list(cx: &mut TestAppContext) {
    let mut run = ready(cx);
    for line in [
        r#"{"t":"undone","op":"trash","ok":false}"#,
        r#"{"t":"redone","op":"trash","ok":false}"#,
    ] {
        run.receive(Backend::test_reply_line(line));
    }
    run.quiesce();
    assert_eq!(lists(&run), 0);
    run.agree("unsuccessful receipts leave the listing alone");
    run.shell
        .update(run.cx, |shell, _| shell.stop_for_app_quit());
}
