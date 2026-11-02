//! Clipboard regressions through mounted Shell input, transport parsing and operation replies.
use super::{edit, frame, key, open, reply, Handle};
use crate::g2::dg1::join::PaneWire;
use gpui::TestAppContext;

#[path = "sol_clipboard_surfaces.rs"]
mod surfaces;

fn changed(cx: &mut TestAppContext, handle: Handle, op: &str, path: &str, token: &str) {
    let line = format!(
        r#"{{"t":"clip","op":"changed","clip":"{op}","paths":["{}"],"token":"{}"}}"#,
        flea::json::escape(path),
        flea::json::escape(token),
    );
    reply(cx, handle, &line);
}

fn copy(cx: &mut TestAppContext, handle: Handle, moving: bool) {
    key(cx, handle, if moving { "ctrl-x" } else { "ctrl-c" });
    reply(cx, handle, r#"{"t":"paths","paths":["/files/a.txt"]}"#);
}

fn running(cx: &mut TestAppContext, handle: Handle) {
    reply(
        cx,
        handle,
        r#"{"t":"transferstarted","id":7,"n":1,"moving":false}"#,
    );
}

#[gpui::test]
fn sol_clipboard_startup_waits_for_the_first_selection(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    edit(cx, handle, |shell, _, _| {
        let wire = PaneWire(shell.panes[0].as_ref().unwrap().backend.as_ref());
        shell.clipboard.board.watch(0, &wire);
        assert!(
            shell.clipboard_menu_facts(0).1,
            "Paste stays enabled before the initial offer"
        );
    });
    lines();
    key(cx, handle, "ctrl-v");
    let sent = lines();
    assert!(sent.iter().any(|line| line.contains("clipGet")), "{sent:?}");
    assert!(
        !sent.iter().any(|line| line.contains("pasteimage")),
        "never guess empty before reading"
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"get","ok":true,"clip":"copy","paths":["/other/first"],"token":"F"}"#,
    );
    assert!(lines()
        .iter()
        .any(|line| line.contains("collisions") && line.contains("/other/first")));
}

#[gpui::test]
fn sol_clipboard_backend_end_releases_unanswered_set(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    copy(cx, handle, false);
    edit(cx, handle, |shell, _, _| shell.clipboard_backend_ended(0));
    lines();
    key(cx, handle, "ctrl-v");
    let sent = lines();
    assert!(
        sent.iter().any(|line| line.contains("clipGet")),
        "a dead publisher cannot suppress fresh reads: {sent:?}"
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"get","ok":true,"clip":"copy","paths":["/other/new"],"token":"N"}"#,
    );
    assert!(lines()
        .iter()
        .any(|line| line.contains("collisions") && line.contains("/other/new")));
}

#[gpui::test]
fn sol_clipboard_backend_end_releases_unanswered_get(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    key(cx, handle, "ctrl-v");
    assert!(lines().iter().any(|line| line.contains("clipGet")));
    edit(cx, handle, |shell, _, _| shell.clipboard_backend_ended(0));
    key(cx, handle, "ctrl-v");
    assert!(
        lines().iter().any(|line| line.contains("clipGet")),
        "a dead read must not leave Paste stuck busy"
    );
}

fn busy_paste(cx: &mut TestAppContext, files: bool) {
    let (handle, mut lines) = open(cx);
    if files {
        changed(cx, handle, "copy", "/other/a", "F");
    } else {
        reply(
            cx,
            handle,
            r#"{"t":"clip","op":"changed","clip":"none","paths":[],"token":""}"#,
        );
    }
    running(cx, handle);
    lines();
    key(cx, handle, "ctrl-v");
    let status = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().status.to_string()
    });
    assert_eq!(status, "An operation is already running.", "files={files}");
    let sent = lines();
    assert!(
        !sent.iter().any(|line| line.contains("collisions")
            || line.contains("transfer")
            || line.contains("pasteimage")),
        "{sent:?}"
    );
}

#[gpui::test]
fn sol_clipboard_running_job_refuses_file_paste(cx: &mut TestAppContext) {
    busy_paste(cx, true);
}

#[gpui::test]
fn sol_clipboard_running_job_refuses_image_paste(cx: &mut TestAppContext) {
    busy_paste(cx, false);
}

#[gpui::test]
fn sol_clipboard_history_reoffers_own_and_foreign_items(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    copy(cx, handle, false);
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"set","ok":true,"token":"T"}"#,
    );
    changed(cx, handle, "copy", "/other/B", "B");
    changed(cx, handle, "copy", "/files/a.txt", "T");
    lines();
    key(cx, handle, "ctrl-v");
    assert!(lines()
        .iter()
        .any(|line| line.contains("collisions") && line.contains("/files/a.txt")));
    reply(
        cx,
        handle,
        r#"{"t":"collisions","id":1,"total":1,"names":[],"mergeable":false}"#,
    );
    key(cx, handle, "escape");
    changed(cx, handle, "copy", "/other/B", "B");
    lines();
    key(cx, handle, "ctrl-v");
    assert!(lines()
        .iter()
        .any(|line| line.contains("collisions") && line.contains("/other/B")));
}

#[gpui::test]
fn sol_clipboard_cut_is_consumed_once_and_copy_can_repeat(cx: &mut TestAppContext) {
    for moving in [true, false] {
        let (handle, mut lines) = open(cx);
        copy(cx, handle, moving);
        reply(
            cx,
            handle,
            r#"{"t":"clip","op":"set","ok":true,"token":"T"}"#,
        );
        changed(
            cx,
            handle,
            if moving { "cut" } else { "copy" },
            "/files/a.txt",
            "T",
        );
        lines();
        key(cx, handle, "ctrl-v");
        assert!(lines().iter().any(|line| line.contains("collisions")));
        reply(
            cx,
            handle,
            r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
        );
        let sent = lines();
        assert!(sent.iter().any(|line| line.contains("transfer")));
        assert_eq!(sent.iter().any(|line| line.contains("clipClear")), moving);
        reply(
            cx,
            handle,
            r#"{"t":"transferstarted","id":7,"n":1,"moving":false}"#,
        );
        reply(
            cx,
            handle,
            r#"{"t":"transferdone","id":7,"ok":1,"failed":0,"skipped":0,"cancelled":false,"retryPaths":[],"durable":false,"note":""}"#,
        );
        // The completed job refreshes the held listing before another paste.
        reply(cx, handle, r#"{"t":"listed","path":"/files","n":0}"#);
        reply(
            cx,
            handle,
            r#"{"t":"rows","start":0,"listing":4,"rows":[]}"#,
        );
        lines();
        key(cx, handle, "ctrl-v");
        let sent = lines();
        assert_eq!(
            sent.iter().any(|line| line.contains("collisions")),
            !moving,
            "{sent:?}"
        );
        assert!(!edit(cx, handle, |shell, _, _| shell
            .clipboard
            .board
            .current()
            .moving));
    }
}

fn foreign_window(cx: &mut TestAppContext, moving: bool) {
    let (source, mut source_lines) = open(cx);
    let (dest, mut dest_lines) = open(cx);
    copy(cx, source, moving);
    let op = if moving { "cut" } else { "copy" };
    let sent = source_lines();
    assert!(sent
        .iter()
        .any(|line| line.contains("clipSet") && line.contains(op)));
    reply(
        cx,
        source,
        r#"{"t":"clip","op":"set","ok":true,"token":"W"}"#,
    );
    changed(cx, dest, op, "/files/a.txt", "W");
    dest_lines();
    key(cx, dest, "ctrl-v");
    assert!(dest_lines()
        .iter()
        .any(|line| line.contains("collisions") && line.contains("/files/a.txt")));
    reply(
        cx,
        dest,
        r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
    );
    let sent = dest_lines();
    let transfer = if moving {
        r#""op":"move""#
    } else {
        r#""op":"copy""#
    };
    assert!(sent.iter().any(|line| line.contains(transfer)));
    assert_eq!(
        sent.iter()
            .any(|line| line.contains("clipClear") && line.contains('W')),
        moving
    );
    if moving {
        reply(
            cx,
            source,
            r#"{"t":"clip","op":"changed","clip":"none","paths":[],"token":""}"#,
        );
    }
    assert_eq!(
        edit(cx, source, |shell, _, _| shell.clipboard.board.available()),
        !moving
    );
    frame(cx, source);
}

#[gpui::test]
fn sol_clipboard_copy_to_another_window(cx: &mut TestAppContext) {
    foreign_window(cx, false);
}

#[gpui::test]
fn sol_clipboard_cut_to_another_window(cx: &mut TestAppContext) {
    foreign_window(cx, true);
}

#[gpui::test]
fn sol_clipboard_unicode_uri_result_reaches_transfer_unchanged(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    changed(cx, handle, "copy", "/other/space name/中文 café.txt", "");
    lines();
    key(cx, handle, "ctrl-v");
    assert!(lines()
        .iter()
        .any(|line| line.contains("/other/space name/中文 café.txt")));
    reply(
        cx,
        handle,
        r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
    );
    assert!(lines()
        .iter()
        .any(|line| line.contains("transfer") && line.contains("/other/space name/中文 café.txt")));
}

#[gpui::test]
fn sol_clipboard_watch_loss_fallback_and_recovery(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    changed(cx, handle, "copy", "/old/a", "old");
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"none","error":"connection lost"}"#,
    );
    assert!(edit(cx, handle, |shell, _, _| shell
        .clipboard_menu_facts(0)
        .1));
    lines();
    key(cx, handle, "ctrl-v");
    assert!(lines().iter().any(|line| line.contains("clipGet")));
    changed(cx, handle, "copy", "/recovered/b", "new");
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"get","ok":true,"clip":"copy","paths":["/stale/a"],"token":"stale"}"#,
    );
    let sent = lines();
    assert!(
        sent.iter()
            .any(|line| line.contains("collisions") && line.contains("/recovered/b")),
        "{sent:?}"
    );
    assert!(!edit(cx, handle, |shell, _, _| shell
        .clipboard_menu_facts(0)
        .1));
}

#[gpui::test]
fn sol_clipboard_garbled_reply_preserves_selection(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    changed(cx, handle, "copy", "/other/a", "F");
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            crate::backend::Backend::test_wire_line(r#"{"t":"clip","op":"changed","paths":["#),
            window,
            cx,
        );
    });
    frame(cx, handle);
    lines();
    key(cx, handle, "ctrl-v");
    assert!(lines()
        .iter()
        .any(|line| line.contains("collisions") && line.contains("/other/a")));
}

#[gpui::test]
fn sol_clipboard_history_reoffer_after_fallback_read(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    copy(cx, handle, false);
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"set","ok":true,"token":"T"}"#,
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","error":"watch down"}"#,
    );
    key(cx, handle, "ctrl-v");
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"get","ok":true,"clip":"copy","paths":["/other/B"],"token":"B"}"#,
    );
    reply(
        cx,
        handle,
        r#"{"t":"collisions","id":1,"total":1,"names":[],"mergeable":false}"#,
    );
    key(cx, handle, "escape");
    changed(cx, handle, "copy", "/files/a.txt", "T");
    lines();
    key(cx, handle, "ctrl-v");
    assert!(
        lines()
            .iter()
            .any(|line| line.contains("collisions") && line.contains("/files/a.txt")),
        "history picks also work after a fallback foreign read"
    );
}
