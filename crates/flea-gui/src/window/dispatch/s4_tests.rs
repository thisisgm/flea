//! SOL-S4: running transfers leave navigation live, refuse another paste, and retain select-all during a sort.
use super::test_shell::{asked, land, open_root, row, sent, status};
use crate::{backend::Backend, state::Shell};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn line(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Backend::test_reply_line(line), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn copying(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("empty", true), row("other", true)],
        1,
    );
    line(
        &shell,
        cx,
        r#"{"t":"clip","op":"changed","clip":"copy","paths":["/source/slow-copy.bin"],"token":"foreign"}"#,
    );
    sent();
    cx.simulate_keystrokes("ctrl-v");
    assert!(sent()
        .iter()
        .any(|line| line.contains(r#""c":"collisions""#)));
    line(
        &shell,
        cx,
        r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
    );
    assert!(sent().iter().any(|line| line.contains(r#""c":"transfer""#)));
    line(
        &shell,
        cx,
        r#"{"t":"transferstarted","id":7,"n":1,"moving":false}"#,
    );
    line(
        &shell,
        cx,
        r#"{"t":"jobs","jobs":[{"id":7,"kind":"copy","head":"Copying 1 of 1","state":"running"}]}"#,
    );
    (shell, cx)
}

#[gpui::test]
fn s4_running_copy_keeps_return_navigation(cx: &mut TestAppContext) {
    let (shell, cx) = copying(cx);
    cx.simulate_keystrokes("home enter");
    assert_eq!(asked(&shell, cx), "/files/empty");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn s4_running_copy_keeps_dispatched_navigation(cx: &mut TestAppContext) {
    let (shell, cx) = copying(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.run_builtin_action("open", window, cx).code, 0);
        })
    });
    assert_eq!(asked(&shell, cx), "/files/empty");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn s4_second_paste_refuses_with_the_oracle_sentence(cx: &mut TestAppContext) {
    let (shell, cx) = copying(cx);
    sent();
    cx.simulate_keystrokes("ctrl-v");
    assert_eq!(status(&shell, cx), "An operation is already running.");
    assert!(!sent().iter().any(|line| {
        line.contains(r#""c":"collisions""#) || line.contains(r#""c":"transfer""#)
    }));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn s4_select_all_during_held_sort_survives_the_landing(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("a", false), row("b", false)],
        1,
    );
    sent();
    cx.simulate_keystrokes("s ctrl-a");
    assert!(sent().iter().any(|line| line.contains(r#""c":"sort""#)));
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .selected
            .clone()),
        vec![0, 1]
    );
    land(
        &shell,
        cx,
        "/files",
        vec![row("b", false), row("a", false)],
        2,
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .selected
            .clone()),
        vec![0, 1]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn s4_partial_marks_still_clear_on_sort_landing(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("a", false), row("b", false)],
        1,
    );
    cx.simulate_keystrokes("s v");
    land(
        &shell,
        cx,
        "/files",
        vec![row("b", false), row("a", false)],
        2,
    );
    assert!(shell.read_with(cx, |shell, _| shell.panes[0]
        .as_ref()
        .unwrap()
        .selected
        .is_empty()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn s4_select_all_does_not_follow_a_new_folder(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("a", true), row("b", false)],
        1,
    );
    cx.simulate_keystrokes("s ctrl-a");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell
                .step(
                    0,
                    crate::window::navigate::Step::Open("/other".into()),
                    window,
                    cx,
                )
                .unwrap();
        })
    });
    land(
        &shell,
        cx,
        "/other",
        vec![row("x", false), row("y", false)],
        2,
    );
    assert!(shell.read_with(cx, |shell, _| shell.panes[0]
        .as_ref()
        .unwrap()
        .selected
        .is_empty()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
