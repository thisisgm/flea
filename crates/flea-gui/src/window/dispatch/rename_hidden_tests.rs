//! S1B-11: a rename to a dot name says the row is now hidden instead of letting it vanish in silence.
use super::{close, draw, notice, ready, receive, sent};
use crate::backend::Event as BackendEvent;
use gpui::TestAppContext;

#[gpui::test]
fn s1b_11_a_dot_name_says_it_is_hidden_while_hidden_files_are_off(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let hidden = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.folders.borrow().hidden
    });
    assert!(!hidden, "hidden files start off");
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input(".notes.txt");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        sent(&mut lines),
        [r#"{"c":"rename","path":"/files/notes.txt","to":".notes.txt","menuId":0}"#]
    );
    receive(
        &shell,
        cx,
        BackendEvent::Producer(r#"{"t":"renamed","ok":true,"path":"/files/.notes.txt"}"#.into()),
    );
    assert_eq!(
        notice(&shell, cx),
        ".notes.txt is hidden by the file filter or the hidden files setting."
    );
    close(&shell, cx);
}

#[gpui::test]
fn s1b_11_a_visible_name_says_nothing_new(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_input("draft");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        sent(&mut lines),
        [r#"{"c":"rename","path":"/files/notes.txt","to":"draft.txt","menuId":0}"#]
    );
    receive(
        &shell,
        cx,
        BackendEvent::Producer(r#"{"t":"renamed","ok":true,"path":"/files/draft.txt"}"#.into()),
    );
    assert_eq!(notice(&shell, cx), "");
    close(&shell, cx);
}

#[gpui::test]
fn s1b_11_a_dot_name_says_nothing_while_hidden_files_show(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    shell.update(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .folders
            .borrow_mut()
            .hidden = true;
    });
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input(".notes.txt");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        sent(&mut lines),
        [r#"{"c":"rename","path":"/files/notes.txt","to":".notes.txt","menuId":0}"#]
    );
    receive(
        &shell,
        cx,
        BackendEvent::Producer(r#"{"t":"renamed","ok":true,"path":"/files/.notes.txt"}"#.into()),
    );
    assert_eq!(
        notice(&shell, cx),
        "",
        "the row stays in sight, so nothing is said"
    );
    close(&shell, cx);
}
