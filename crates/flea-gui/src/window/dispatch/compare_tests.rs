//! Compare opens the Quick Look card on two selected regular files: the patch, each refusal, a still cursor, Copy Patch and Escape.
use super::{
    preview_tests::{answer, body, close, context, draw, file, opened, sent, token, Lines},
    test_shell::{land, row},
};
use crate::{g2::pv1::Body, state::Shell, window::u07_tests::read};
use flea::{
    json::escape,
    jsondoc::{self, Json},
    keys::Context,
};
use gpui::{Entity, VisualTestContext};

const PATCH: &str = "--- /files/notes-a.txt\n+++ /files/notes-b.txt\n@@ -1,3 +1,3 @@\n Compare fixture\n-line three\n+line three changed in b\n line four\n";

fn pair(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let rows = vec![
        file("notes-a.txt", "text-plain"),
        file("notes-b.txt", "text-plain"),
        row("folder", true),
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
fn compare(
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
    assert!(sent(&mut requests, "code").is_empty() && sent(&mut requests, "meta").is_empty());
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
    for rows in [&[0][..], &[0, 2], &[0, 1, 2]] {
        select(&shell, cx, rows);
        let opened =
            cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_compare(0, window, cx)));
        assert!(!opened, "{rows:?} is no pair of regular files");
        assert!(sent(&mut requests, "compare").is_empty());
        assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    }
    close(&shell, cx);
}
