//! Clipboard outcomes through mounted menu input and the transfer reply path.
use super::{edit, frame, key, open, reply, Handle};
use gpui::{point, px, ClipboardItem, MouseButton, TestAppContext, VisualTestContext};
use std::time::Duration;

#[path = "sol_clipboard_tests.rs"]
mod sol_clipboard_tests;

#[path = "image_paste_tests.rs"]
mod image_paste_tests;

const FORMS: [(&str, &str); 7] = [
    ("copyPath", "/files/a.txt\n/files/b.txt"),
    ("copyRelative", "a.txt\nb.txt"),
    ("copyName", "a.txt\nb.txt"),
    ("copyStem", "a\nb"),
    ("copydirpath", "/files\n/files"),
    ("copyUri", "file:///files/a.txt\nfile:///files/b.txt"),
    ("copyQuoted", "'/files/a.txt'\n'/files/b.txt'"),
];

// The listing key opens the cursor menu; its snapshot and applications answers ready the rows.
fn menu(
    cx: &mut TestAppContext,
    handle: Handle,
    lines: &mut impl FnMut() -> Vec<String>,
    press: &str,
) -> usize {
    key(cx, handle, press);
    let id = lines()
        .iter()
        .find_map(|line| {
            line.contains(r#""op":"snapshot""#)
                .then(|| flea::json::field_usize(line, "id"))
                .flatten()
        })
        .expect("the menu captures the selected rows");
    for op in ["snapshot", "applications"] {
        reply(
            cx,
            handle,
            &format!(
                r#"{{"t":"menuaction","id":{id},"op":"{op}","ok":true,"kinds":[32768,32768],"applications":[]}}"#
            ),
        );
    }
    lines();
    id
}

fn copy_as(cx: &mut TestAppContext, pointer: bool, shown: bool) {
    let (handle, mut lines) = open(cx);
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 1];
        shell.launch.settings.data = flea::jsondoc::parse(if shown {
            r#"{"keys":"default","menu":{"hidden":[]}}"#
        } else {
            r#"{"keys":"default","menu":{"hidden":["copyAs"]}}"#
        })
        .unwrap();
    });
    lines();
    for (index, (leaf, expected)) in FORMS.iter().enumerate() {
        cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string("sentinel".into())));
        let id = menu(cx, handle, &mut lines, "c");
        if pointer {
            let at = edit(cx, handle, |shell, _, _| {
                let menu = shell.menu.menu.borrow();
                let card = &menu.flyout.as_ref().expect("c opens Copy As").card;
                point(
                    px(card.frame.x + card.frame.w / 2.),
                    px(card.row_screen_top(index)
                        + flea::ui_logic_context_menus::fit::ROW as f32 / 2.),
                )
            });
            VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
                at,
                MouseButton::Left,
                Default::default(),
            );
            frame(cx, handle);
        } else {
            for _ in 0..index {
                key(cx, handle, "down");
            }
            key(cx, handle, "enter");
        }
        let chosen = format!("copyAs:{leaf}");
        let sent = lines();
        assert!(
            sent.iter()
                .any(|line| line.contains(r#""op":"validate""#) && line.contains(&chosen)),
            "EY-164 {chosen} must reach validation: {sent:?}"
        );
        reply(
            cx,
            handle,
            &format!(
                r#"{{"t":"menuaction","id":{id},"op":"validate","ok":true,"action":"{chosen}","paths":["/files/a.txt","/files/b.txt"]}}"#
            ),
        );
        let actual = cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(actual.as_deref(), Some(*expected), "EY-164 {chosen}");
        assert!(
            lines()
                .iter()
                .all(|line| !line.contains(r#""c":"clipSet""#)),
            "Copy As writes text without claiming a file selection"
        );
    }
}

#[gpui::test]
fn pb_clipboard_ey164_return_copies_every_form_with_hidden_parent(cx: &mut TestAppContext) {
    copy_as(cx, false, false);
}

#[gpui::test]
fn pb_clipboard_ey164_pointer_copies_every_form_with_hidden_parent(cx: &mut TestAppContext) {
    copy_as(cx, true, false);
}

#[gpui::test]
fn pb_clipboard_ey164_return_copies_every_form_with_shown_parent(cx: &mut TestAppContext) {
    copy_as(cx, false, true);
}

#[gpui::test]
fn pb_clipboard_ey164_pointer_copies_every_form_with_shown_parent(cx: &mut TestAppContext) {
    copy_as(cx, true, true);
}

// Keep Both into a transfer that completes; returns the lines sent once it has finished.
fn keep_both_completes(
    cx: &mut TestAppContext,
    handle: Handle,
    lines: &mut impl FnMut() -> Vec<String>,
) -> Vec<String> {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.cursor = 1;
        host.selected = vec![1];
    });
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"copy","paths":["/other/a.txt"],"token":"T1"}"#,
    );
    lines();
    key(cx, handle, "ctrl-v");
    let id = lines()
        .iter()
        .find_map(|line| {
            line.contains(r#""c":"collisions""#)
                .then(|| flea::json::field_usize(line, "id"))
                .flatten()
        })
        .expect("paste asks for collisions");
    reply(
        cx,
        handle,
        &format!(
            r#"{{"t":"collisions","id":{id},"total":1,"names":[{{"n":"a.txt","d":false,"i":"text-plain"}}],"mergeable":false}}"#
        ),
    );
    key(cx, handle, "enter");
    assert!(
        lines()
            .iter()
            .any(|line| line.contains(r#""c":"transfer""#) && line.contains(r#""collide":"keep""#)),
        "Return chooses Keep Both"
    );
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
    lines()
}

// The status line the footer reads from the pane.
fn footer_status(cx: &mut TestAppContext, handle: Handle) -> String {
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().status.to_string()
    })
}

#[gpui::test]
fn pb_clipboard_ey169_keep_both_refreshes_and_keeps_the_place(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    let sent = keep_both_completes(cx, handle, &mut lines);
    assert!(
        sent.iter().any(|line| line.contains(r#""c":"list""#)),
        "EY-169 completion refreshes the listing: {sent:?}"
    );
    reply(cx, handle, r#"{"t":"listed","path":"/files","n":4}"#);
    reply(
        cx,
        handle,
        r#"{"t":"rows","start":0,"listing":4,"rows":[{"n":"a (copy).txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"},{"n":"a.txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"},{"n":"b.txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"},{"n":"c.txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"}]}"#,
    );
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.pane.busy());
        assert_eq!(host.pane.cursor, 2, "the cursor stays on b.txt");
        assert_eq!(host.selected, vec![2], "b.txt stays marked");
        assert_eq!(host.state_text.status, "4 items");
    });
}

#[gpui::test]
fn pb_clipboard_ey169_footer_keeps_the_completion_line_until_the_folder_changes(
    cx: &mut TestAppContext,
) {
    let (handle, mut lines) = open(cx);
    keep_both_completes(cx, handle, &mut lines);
    let completed = footer_status(cx, handle);
    assert!(!completed.is_empty(), "the completion writes its line");
    reply(cx, handle, r#"{"t":"listed","path":"/files","n":4}"#);
    reply(
        cx,
        handle,
        r#"{"t":"rows","start":0,"listing":4,"rows":[{"n":"a (copy).txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"},{"n":"a.txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"},{"n":"b.txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"},{"n":"c.txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"}]}"#,
    );
    assert_eq!(
        footer_status(cx, handle),
        completed,
        "the completion line holds across its own refresh"
    );
    key(cx, handle, "backspace");
    reply(cx, handle, r#"{"t":"listed","path":"/","n":3}"#);
    reply(
        cx,
        handle,
        r#"{"t":"rows","start":0,"listing":5,"rows":[{"n":"a.txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"},{"n":"b.txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"},{"n":"c.txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"}]}"#,
    );
    assert_eq!(
        footer_status(cx, handle),
        "3 items",
        "another folder shows its own count"
    );
}

#[gpui::test]
fn pb_clipboard_ey169_the_completion_line_ends_with_its_notice(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    keep_both_completes(cx, handle, &mut lines);
    reply(cx, handle, r#"{"t":"listed","path":"/files","n":4}"#);
    reply(
        cx,
        handle,
        r#"{"t":"rows","start":0,"listing":4,"rows":[{"n":"a (copy).txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"},{"n":"a.txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"},{"n":"b.txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"},{"n":"c.txt","d":false,"s":0,"m":0,"p":33188,"i":"text-plain"}]}"#,
    );
    assert_ne!(footer_status(cx, handle), "4 items");
    cx.executor().advance_clock(Duration::from_millis(4_100));
    cx.run_until_parked();
    assert_eq!(
        footer_status(cx, handle),
        "4 items",
        "the notice's display time ends the completion line"
    );
}

#[gpui::test]
fn pb_clipboard_paste_as_hidden_leaves_reach_the_file_clipboard(cx: &mut TestAppContext) {
    for (index, leaf) in ["pasteLink", "pasteAbsoluteLink", "pasteHardLink"]
        .iter()
        .enumerate()
    {
        // Each leaf starts in a fresh window: the collision ask a paste leaves open would refuse the next one.
        let (handle, mut lines) = open(cx);
        edit(cx, handle, |shell, _, _| {
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"keys":"default","menu":{"hidden":["pasteAs"]}}"#)
                    .unwrap();
        });
        reply(
            cx,
            handle,
            r#"{"t":"clip","op":"changed","clip":"copy","paths":["/other/a.txt"],"token":"T1"}"#,
        );
        lines();
        let id = menu(cx, handle, &mut lines, "shift-p");
        for _ in 0..index {
            key(cx, handle, "down");
        }
        key(cx, handle, "enter");
        let chosen = format!("pasteAs:{leaf}");
        let sent = lines();
        assert!(
            sent.iter()
                .any(|line| line.contains(r#""op":"validate""#) && line.contains(&chosen)),
            "Paste As validates {chosen}: {sent:?}"
        );
        reply(
            cx,
            handle,
            &format!(
                r#"{{"t":"menuaction","id":{id},"op":"validate","ok":true,"action":"{chosen}","paths":["/files/b.txt"]}}"#
            ),
        );
        let asked = lines();
        assert!(
            asked.iter().any(|line| line.contains("/other/a.txt")),
            "Paste As uses the file clipboard rather than the menu snapshot: {asked:?}"
        );
    }
}

// EY-360: a finished paste says "Copied 1 item" with its undo advice as the footer's own target, so undoAvailable reads true.
#[gpui::test]
fn pb_clipboard_ey360_transfer_receipt_keeps_its_undo_target(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    keep_both_completes(cx, handle, &mut lines);
    let (primary, undo) = edit(cx, handle, |shell, _, cx| {
        let footer = shell.surfaces[0]
            .footer
            .clone()
            .expect("the footer is mounted");
        let line = footer.read(cx).message_line();
        let drawn = line.read(cx).drawn().clone();
        (drawn.primary.to_string(), drawn.undo.to_string())
    });
    assert_eq!(
        (primary.as_str(), undo.as_str()),
        ("Copied 1 item", "z undoes")
    );
}
