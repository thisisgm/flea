//! The slow click through real presses (SlowClick.js arm and fire, a double click, a press off the name) and the rename readers.
use super::rename_tests::{close, draw, editing, ready};
use crate::{
    state::Shell,
    window::u07_tests::{press, read},
};
use gpui::{Entity, VisualTestContext};
use std::time::Duration;

// A point on the drawn name of the row, and one on the row past its name.
fn name_and_past(shell: &Entity<Shell>, cx: &mut VisualTestContext, row: &str) -> (String, String) {
    draw(cx);
    let numbers = |key: &str, cx: &mut VisualTestContext| -> Vec<f32> {
        let rect = read(shell, cx, key, &[row]);
        rect.split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect()
    };
    let name = numbers("rowNameRect", cx);
    let whole = numbers("rowRect", cx);
    let middle = name[1] + name[3] / 2.;
    (
        format!("{} {middle}", name[0] + 4.),
        format!("{} {middle}", whole[0] + whole[2] - 8.),
    )
}

fn wait(cx: &mut VisualTestContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.run_until_parked();
}

#[gpui::test]
fn a_second_slow_tap_on_the_selected_name_opens_the_editor(cx: &mut gpui::TestAppContext) {
    let (shell, cx, _) = ready(cx, "default");
    let (name, _) = name_and_past(&shell, cx, "2");
    press(cx, &name, 1);
    wait(cx, 600);
    assert_eq!(editing(&shell, cx), None, "the first tap only selects");
    press(cx, &name, 1);
    wait(cx, 100);
    assert_eq!(
        editing(&shell, cx),
        None,
        "the editor waits for the interval"
    );
    wait(cx, 400);
    draw(cx);
    assert_eq!(
        editing(&shell, cx),
        Some((2, "notes.txt".into(), "notes".into())),
        "the slow second tap opens the rename on its row"
    );
    // Ipc.qml's rename readers report the open editor.
    assert_eq!(read(&shell, cx, "renamingIndex", &[]), "2");
    assert_eq!(read(&shell, cx, "renameEditorLive", &[]), "true");
    assert_eq!(read(&shell, cx, "renameEditorText", &[]), "notes.txt");
    let state = read(&shell, cx, "renameState", &[]);
    assert!(state.contains(r#""selectedText":"notes""#), "{state}");
    assert_eq!(
        read(&shell, cx, "bulkRenameState", &[]),
        r#"{"opened":false}"#
    );
    close(&shell, cx);
}

#[gpui::test]
fn a_double_click_or_a_press_past_the_name_never_renames(cx: &mut gpui::TestAppContext) {
    let (shell, cx, _) = ready(cx, "default");
    let (name, past) = name_and_past(&shell, cx, "2");
    press(cx, &name, 1);
    wait(cx, 600);
    press(cx, &past, 1);
    wait(cx, 600);
    assert_eq!(editing(&shell, cx), None, "a tap off the name disarms");
    press(cx, &name, 1);
    wait(cx, 600);
    assert_eq!(
        editing(&shell, cx),
        None,
        "the tap after the disarm arms again and never renames"
    );
    press(cx, &name, 1);
    press(cx, &name, 2);
    wait(cx, 600);
    assert_eq!(
        editing(&shell, cx),
        None,
        "a double click opens, never renames"
    );
    close(&shell, cx);
}
