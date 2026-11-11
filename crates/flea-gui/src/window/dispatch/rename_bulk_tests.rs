//! The bulk rename card: several marked rows open it, Enter applies, its done line refreshes, and Escape sends nothing.
use super::rename_tests::{close, draw, editing, notice, ready, ready_root, receive, sent};
use super::test_shell::lists;
use crate::backend::Event as BackendEvent;
use gpui::TestAppContext;

#[gpui::test]
fn several_marked_rows_open_the_bulk_card_and_its_done_line_refreshes(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready_root(cx, "default");
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![1, 2];
        host.input.selection.changed(&host.selected);
    });
    cx.simulate_keystrokes("f2");
    assert_eq!(
        editing(&shell, cx),
        None,
        "several rows never open the inline editor"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.rename.paths),
        Some(0),
        "the paths are asked"
    );
    receive(
        &shell,
        cx,
        BackendEvent::Paths(vec!["/files/beta".into(), "/files/notes.txt".into()]),
    );
    assert!(
        shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)),
        "the card opens"
    );
    draw(cx);
    // Find "e", replace it with "x": both names change, and Enter on Replace renames.
    cx.simulate_input("e");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("x");
    cx.simulate_keystrokes("enter");
    let out = sent(&mut lines);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(
        out[0].starts_with(r#"{"c":"bulkrename""#)
            && out[0].contains("bxta")
            && out[0].contains("notxs.txt"),
        "{out:?}"
    );
    assert!(
        !shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)),
        "the card closed"
    );
    let before = lists(&shell, cx);
    receive(
        &shell,
        cx,
        BackendEvent::Producer(r#"{"t":"bulkrenamed","ok":2,"unchanged":0}"#.into()),
    );
    let done = shell.read_with(cx, |shell, _| {
        flea::ui_logic_file_ops::lines::bulk_done_line(&shell.locale.catalog, "default", 2)
    });
    assert_eq!(notice(&shell, cx), done);
    assert_eq!(lists(&shell, cx), before + 1);
    assert!(shell.read_with(cx, |shell, _| shell.panes[0]
        .as_ref()
        .unwrap()
        .selected
        .is_empty()));
    close(&shell, cx);
}

#[gpui::test]
fn escape_closes_the_bulk_card_and_sends_nothing(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 1]
    });
    cx.simulate_keystrokes("f2");
    receive(
        &shell,
        cx,
        BackendEvent::Paths(vec!["/files/alpha".into(), "/files/beta".into()]),
    );
    assert!(
        shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)),
        "the card opened"
    );
    cx.simulate_keystrokes("escape");
    assert!(!shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)));
    assert_eq!(sent(&mut lines), Vec::<String>::new());
    close(&shell, cx);
}
