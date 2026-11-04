//! Opening settings and terminal keys through the mounted production window.
use super::test_shell::{land, open_root_with, row};
use crate::{
    state::Shell,
    window::u07_tests::{press, read},
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, time::Duration};

thread_local! {
    static LAUNCHED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}

fn ready<'a>(
    cx: &'a mut TestAppContext,
    settings: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = open_root_with(cx, settings, None);
    land(
        &shell,
        cx,
        "/files",
        vec![row("folder", true), row("notes.txt", false)],
        1,
    );
    LAUNCHED.with(|calls| calls.borrow_mut().clear());
    shell.update(cx, |shell, _| {
        for lane in &mut shell.opener {
            lane.launch = |argv| {
                LAUNCHED.with(|calls| calls.borrow_mut().push(argv.to_vec()));
                (Some(0), String::new())
            };
        }
    });
    (shell, cx)
}

fn calls() -> Vec<Vec<String>> {
    LAUNCHED.with(|calls| calls.take())
}

#[gpui::test]
fn pb_opener_ctrl_t_opens_the_shown_folder(cx: &mut TestAppContext) {
    let (shell, cx) = ready(cx, r#""keys":"default","view":"list""#);
    cx.simulate_keystrokes("down ctrl-t");
    let calls = calls();
    assert_eq!(calls.len(), 1, "Ctrl+T must launch the terminal");
    assert_eq!(&calls[0][1..], ["--terminal", "/files"]);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_opener_single_click_setting_opens_once_and_never_renames(cx: &mut TestAppContext) {
    let (shell, cx) = ready(cx, r#""keys":"default","view":"list","openMode":"single""#);
    let name = read(&shell, cx, "rowNameRect", &["1"]);
    let rect: Vec<f32> = name
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    let at = format!("{} {}", rect[0] + 4., rect[1] + rect[3] / 2.);
    press(cx, &at, 1);
    let opened = calls();
    assert_eq!(
        opened.len(),
        1,
        "one tap must obey Settings > View > Single click"
    );
    assert_eq!(&opened[0][1..], ["--open", "/files/notes.txt"]);
    press(cx, &at, 2);
    assert!(
        calls().is_empty(),
        "the second tap of a double click must not open twice"
    );
    cx.executor().advance_clock(Duration::from_millis(600));
    press(cx, &at, 1);
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert_eq!(read(&shell, cx, "renamingIndex", &[]), "-1");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_opener_double_click_setting_ignores_obsolete_single_click_flag(cx: &mut TestAppContext) {
    let (shell, cx) = ready(
        cx,
        r#""keys":"default","view":"list","openMode":"double","singleClick":true"#,
    );
    let at = read(&shell, cx, "rowCentre", &["1"]);
    press(cx, &at, 1);
    assert!(calls().is_empty(), "the canonical opening setting decides");
    press(cx, &at, 2);
    assert_eq!(calls().len(), 1);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
