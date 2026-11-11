//! Compare opens the Quick Look card on two selected regular files: the patch, each refusal, a still cursor, Copy Patch and Escape.
use super::{
    preview_tests::{answer, body, close, context, drain, draw, file, opened, sent, token, Lines},
    test_shell::{land, land_on, row},
};
use crate::{g2::quick_look_overlay::Body, state::Shell, window::u07_tests::read};
use flea::{
    json::escape,
    jsondoc::{self, Json},
    keys::Context,
    protocol::RowFacts,
};
use gpui::{Entity, VisualTestContext};

const PATCH: &str = "--- /files/notes-a.txt\n+++ /files/notes-b.txt\n@@ -1,3 +1,3 @@\n Compare fixture\n-line three\n+line three changed in b\n line four\n";

pub(super) fn pair(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let rows = vec![
        file("notes-a.txt", "text-plain"),
        file("notes-b.txt", "text-plain"),
        row("folder", true),
        file("notes-c.txt", "text-plain"),
        RowFacts {
            mode: 0o120777,
            ..file("link", "inode-symlink")
        },
    ];
    land(shell, cx, "/files", rows, 1);
    select(shell, cx, &[0, 1]);
}

fn select(shell: &Entity<Shell>, cx: &mut VisualTestContext, rows: &[usize]) {
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = rows.to_vec();
        host.pane.cursor = rows[rows.len() - 1];
    });
}

// Opens Compare through the Shell's own entry point and answers its one ask with `state`.
pub(super) fn compare(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    requests: &mut Lines,
    state: &str,
    patch: &str,
) -> Json {
    let opened =
        cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_compare(0, window, cx)));
    assert!(opened, "two selected regular files open Compare");
    draw(cx);
    let asked = sent(requests, "compare");
    assert_eq!(asked.len(), 1, "one diff asked per open");
    assert_eq!(
        asked[0].get("a").and_then(Json::as_str),
        Some("/files/notes-a.txt")
    );
    assert_eq!(
        asked[0].get("b").and_then(Json::as_str),
        Some("/files/notes-b.txt")
    );
    assert_eq!(read(shell, cx, "previewOpen", &[]), "true");
    assert_eq!(read(shell, cx, "previewKind", &[]), "compare");
    let waiting = state_of(shell, cx);
    assert_eq!(waiting.get("ready").and_then(Json::as_bool), Some(false));
    let line = format!(
        r#"{{"t":"compare","token":{},"state":"{state}","patch":"{}"}}"#,
        token(&asked[0]),
        escape(patch)
    );
    answer(shell, cx, line);
    state_of(shell, cx)
}

fn state_of(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    jsondoc::parse(&read(shell, cx, "compareState", &[])).expect("compareState is JSON")
}

fn text<'a>(doc: &'a Json, key: &str) -> &'a str {
    doc.get(key).and_then(Json::as_str).unwrap_or("")
}

#[gpui::test]
fn compare_shows_the_patch_keeps_the_cursor_still_copies_and_closes(cx: &mut gpui::TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    pair(&shell, cx);
    let before = state_of(&shell, cx);
    assert_eq!(
        before.get("opened").and_then(Json::as_bool),
        Some(false),
        "no card before Compare"
    );
    let state = compare(&shell, cx, &mut requests, "diff", PATCH);
    assert_eq!(state.get("opened").and_then(Json::as_bool), Some(true));
    assert_eq!(text(&state, "answer"), "diff");
    assert_eq!(
        text(&state, "mark"),
        "file-text",
        "the IPC reports the drawn glyph name"
    );
    assert_eq!(
        text(&state, "title"),
        "notes-a.txt and notes-b.txt \u{b7} 2 lines differ"
    );
    let kinds: Vec<&str> = state
        .get("rows")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .map(|r| text(r, "kind"))
        .collect();
    assert_eq!(kinds, ["hunk", "context", "removed", "added", "context"]);
    assert!(matches!(body(&shell, cx), Some(Body::Diff { copy, .. }) if copy == "Copy Patch"));
    assert!(
        cx.debug_bounds("pv1-card").is_some(),
        "the Compare card draws"
    );
    // A Compare card names two files, so the cursor keys open nothing else under it.
    let cursor = read(&shell, cx, "cursor", &[]);
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(read(&shell, cx, "cursor", &[]), cursor);
    // One drain, so a meta ask is never consumed by the read that looks for a code ask.
    let after = drain(&mut requests);
    assert!(
        after
            .iter()
            .all(|doc| !matches!(doc.get("c").and_then(Json::as_str), Some("code" | "meta"))),
        "the card follows nothing: {after:?}"
    );
    assert_eq!(read(&shell, cx, "previewKind", &[]), "compare");
    // y copies the whole patch, headers included.
    cx.simulate_keystrokes("y");
    draw(cx);
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some(PATCH));
    // Tab walks Copy Patch, then Close.
    cx.simulate_keystrokes("tab");
    draw(cx);
    let focused = |state: &Json, name: &str| {
        state
            .get("controls")
            .and_then(Json::as_array)
            .unwrap()
            .iter()
            .any(|c| {
                text(c, "name") == name && c.get("focused").and_then(Json::as_bool) == Some(true)
            })
    };
    assert!(focused(&state_of(&shell, cx), "copy"));
    cx.simulate_keystrokes("tab");
    draw(cx);
    let walked = state_of(&shell, cx);
    assert!(
        focused(&walked, "close") && !focused(&walked, "copy"),
        "the second Tab reaches Close"
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    assert_eq!(
        state_of(&shell, cx).get("opened").and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(context(&shell, cx), Context::Listing);
    close(&shell, cx);
}

#[gpui::test]
fn a_binary_identical_or_large_pair_shows_its_refusal_bar(cx: &mut gpui::TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    pair(&shell, cx);
    for (state, sentence) in [
        (
            "binary",
            "These files are binary, so there is nothing to compare.",
        ),
        ("identical", "The two files are identical."),
        ("over", "These files are over 1.0 MB, too large to compare."),
        ("failed", "These files could not be compared."),
    ] {
        let shown = compare(&shell, cx, &mut requests, state, "");
        assert_eq!(text(&shown, "answer"), state);
        assert_eq!(
            text(&shown, "title"),
            "notes-a.txt and notes-b.txt",
            "{state}: the names alone"
        );
        assert_eq!(text(&shown, "sentence"), sentence);
        assert!(
            matches!(body(&shell, cx), Some(Body::Refused { .. })),
            "{state}"
        );
        assert_eq!(read(&shell, cx, "previewState", &[]), sentence);
        cx.simulate_keystrokes("escape");
        draw(cx);
        assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    }
    close(&shell, cx);
}

#[gpui::test]
fn compare_refuses_anything_but_two_regular_files(cx: &mut gpui::TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    pair(&shell, cx);
    // One file, a folder, three files, three with a folder, and a file with a link.
    for rows in [&[0][..], &[0, 2], &[0, 1, 3], &[0, 1, 2], &[0, 4]] {
        select(&shell, cx, rows);
        let opened =
            cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_compare(0, window, cx)));
        assert!(!opened, "{rows:?} is no pair of regular files");
        assert!(sent(&mut requests, "compare").is_empty());
        assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    }
    close(&shell, cx);
}

fn menu(requests: &mut Lines, op: &str) -> Vec<Json> {
    sent(requests, "menuaction")
        .into_iter()
        .filter(|doc| text(doc, "op") == op)
        .collect()
}

fn reply(shell: &Entity<Shell>, cx: &mut VisualTestContext, id: u64, op: &str, fields: &str) {
    answer(
        shell,
        cx,
        format!(r#"{{"t":"menuaction","id":{id},"op":"{op}","ok":true,{fields}}}"#),
    );
}

// Ten rows more than the first window, so the returned row is one the window has not read.
fn far_listing(shell: &Entity<Shell>, cx: &mut VisualTestContext, requests: &mut Lines) -> usize {
    let first = shell.read_with(cx, |shell, _| {
        let pending = shell.panes[0].as_ref().unwrap().pane.swap.pending.as_ref();
        pending.unwrap().first
    });
    let mut rows = vec![file("notes-a.txt", "text-plain"), row("folder", true)];
    rows.extend((2..first).map(|at| file(&format!("file{at}.txt"), "text-plain")));
    land_on(shell, cx, 0, "/files", rows, first + 10, 1);
    drain(requests);
    first + 5
}

// Ops.js menuKinds: a row outside the held window is named by the backend's snapshot, never refused for being unheld.
#[gpui::test]
fn compare_names_rows_outside_the_held_window_through_the_backend(cx: &mut gpui::TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    let far = far_listing(&shell, cx, &mut requests);
    let open = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_compare(0, window, cx)))
    };
    // A held folder refuses at once, whatever the unheld row is.
    select(&shell, cx, &[1, far]);
    assert!(!open(&shell, cx));
    assert!(drain(&mut requests).is_empty());
    // Two files the backend names regular: snapshot, validate, close, then the one diff.
    select(&shell, cx, &[0, far]);
    assert!(open(&shell, cx), "the snapshot is asked for");
    draw(cx);
    let snapshot = menu(&mut requests, "snapshot");
    assert_eq!(snapshot.len(), 1);
    let rows: Vec<f64> = snapshot[0]
        .get("rows")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .filter_map(Json::as_f64)
        .collect();
    assert_eq!(rows, [0., far as f64]);
    assert_eq!(snapshot[0].get("listing").and_then(Json::as_f64), Some(1.));
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    let id = token_of(&snapshot[0]);
    reply(
        &shell,
        cx,
        id,
        "snapshot",
        r#""count":2,"kinds":[32768,32768]"#,
    );
    let validate = menu(&mut requests, "validate");
    assert_eq!(validate.len(), 1);
    assert_eq!(text(&validate[0], "action"), "compare");
    assert_eq!(token_of(&validate[0]), id);
    reply(
        &shell,
        cx,
        id,
        "validate",
        r#""action":"compare","paths":["/files/notes-a.txt","/files/far.txt"],"dest":"""#,
    );
    let after = drain(&mut requests);
    let ops: Vec<&str> = after.iter().map(|doc| text(doc, "c")).collect();
    assert_eq!(ops, ["menuaction", "compare"], "close, then the diff");
    assert_eq!(text(&after[0], "op"), "close");
    assert_eq!(text(&after[1], "a"), "/files/notes-a.txt");
    assert_eq!(text(&after[1], "b"), "/files/far.txt");
    assert_eq!(read(&shell, cx, "previewKind", &[]), "compare");
    cx.simulate_keystrokes("escape");
    draw(cx);
    // A pair the backend names a file and a link closes the snapshot and opens nothing.
    assert!(open(&shell, cx));
    draw(cx);
    let id = token_of(&menu(&mut requests, "snapshot")[0]);
    reply(
        &shell,
        cx,
        id,
        "snapshot",
        r#""count":2,"kinds":[32768,40960]"#,
    );
    let after = drain(&mut requests);
    assert_eq!(after.len(), 1);
    assert_eq!(text(&after[0], "op"), "close");
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    // A selection that moved while the snapshot was out opens nothing either.
    assert!(open(&shell, cx));
    draw(cx);
    let id = token_of(&menu(&mut requests, "snapshot")[0]);
    select(&shell, cx, &[0, far + 1]);
    reply(
        &shell,
        cx,
        id,
        "snapshot",
        r#""count":2,"kinds":[32768,32768]"#,
    );
    assert!(menu(&mut requests, "validate").is_empty());
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    close(&shell, cx);
}

fn token_of(doc: &Json) -> u64 {
    doc.get("id").and_then(Json::as_f64).expect("an id") as u64
}

// Every snapshot Compare asks for is closed: by a newer Compare, by Space opening a file, and by the card closing.
#[gpui::test]
fn a_superseded_or_closed_compare_snapshot_is_closed(cx: &mut gpui::TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    let far = far_listing(&shell, cx, &mut requests);
    let open = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        assert!(
            cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_compare(0, window, cx)))
        );
        draw(cx);
    };
    let ops = |requests: &mut Lines| -> Vec<(String, u64)> {
        drain(requests)
            .iter()
            .map(|doc| {
                (
                    format!("{} {}", text(doc, "c"), text(doc, "op")),
                    doc.get("id")
                        .or(doc.get("token"))
                        .and_then(Json::as_f64)
                        .unwrap_or(0.) as u64,
                )
            })
            .collect()
    };
    select(&shell, cx, &[0, far]);
    open(&shell, cx);
    let old = token_of(&menu(&mut requests, "snapshot")[0]);
    open(&shell, cx);
    let sent = ops(&mut requests);
    assert_eq!(
        sent[0],
        ("menuaction close".into(), old),
        "the older snapshot closes first"
    );
    assert_eq!(sent[1].0, "menuaction snapshot");
    let pending = sent[1].1;
    // Space on a held file replaces the ask with the card's read.
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 0
    });
    cx.simulate_keystrokes("space");
    draw(cx);
    let sent = ops(&mut requests);
    assert_eq!(sent[0], ("menuaction close".into(), pending));
    assert_eq!(sent[1].0, "code ");
    // A Compare asked while that card is open closes its snapshot when the card closes.
    open(&shell, cx);
    let pending = token_of(&menu(&mut requests, "snapshot")[0]);
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    assert_eq!(
        ops(&mut requests),
        [("menuaction close".to_string(), pending)]
    );
    close(&shell, cx);
}
