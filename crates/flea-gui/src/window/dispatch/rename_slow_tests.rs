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

#[gpui::test]
fn open_mode_single_click_never_arms_or_fires_the_slow_rename(cx: &mut gpui::TestAppContext) {
    let (shell, cx, _) = ready(cx, "default");
    shell.update(cx, |shell, _| {
        // The Settings page greys clickRename out when openMode is "single"; the slow click must refuse it too.
        shell.launch.settings.data = flea::jsondoc::parse(r#"{"openMode":"single"}"#).unwrap();
        // The lone tap opens the file, so the test opener stands in for the desktop's.
        shell.opener[0].launch = |_| (Some(0), String::new());
    });
    let (name, _) = name_and_past(&shell, cx, "2");
    press(cx, &name, 1);
    wait(cx, 600);
    assert_eq!(editing(&shell, cx), None, "the first tap only opens");
    press(cx, &name, 1);
    wait(cx, 100);
    assert!(
        shell.read_with(cx, |shell, _| shell
            .rename
            .slow
            .as_ref()
            .is_none_or(|slow| !slow.pending())),
        "single-click mode never arms the slow click"
    );
    wait(cx, 400);
    draw(cx);
    assert_eq!(
        editing(&shell, cx),
        None,
        "single-click mode never starts the rename"
    );
    close(&shell, cx);
}

#[gpui::test]
fn a_slow_tap_just_past_the_interval_keeps_submillisecond_precision(cx: &mut gpui::TestAppContext) {
    let (shell, cx, _) = ready(cx, "default");
    let (name, _) = name_and_past(&shell, cx, "2");
    press(cx, &name, 1);
    cx.executor()
        .advance_clock(Duration::from_millis(400) + Duration::from_nanos(1));
    press(cx, &name, 1);
    wait(cx, 401);
    draw(cx);
    assert_eq!(
        editing(&shell, cx),
        Some((2, "notes.txt".into(), "notes".into()))
    );
    close(&shell, cx);
}

// EY-282: GridArea.qml arms the same slow click from a lone tap on the painted caption.
#[gpui::test]
fn a_second_slow_tap_on_a_grid_caption_opens_the_editor(cx: &mut gpui::TestAppContext) {
    let (shell, cx, _) = ready(cx, "default");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_view(crate::window::startup::View::Grid, window, cx)
        })
    });
    draw(cx);
    let caption = read(&shell, cx, "rowNameCentre", &["2"]);
    assert!(!caption.is_empty(), "the grid paints row 2's caption");
    press(cx, &caption, 1);
    wait(cx, 600);
    assert_eq!(editing(&shell, cx), None, "the first tap only selects");
    press(cx, &caption, 1);
    wait(cx, 500);
    draw(cx);
    assert_eq!(
        editing(&shell, cx),
        Some((2, "notes.txt".into(), "notes".into())),
        "the slow second tap on the caption opens the rename with the stem selected"
    );
    assert_eq!(read(&shell, cx, "renameEditorLive", &[]), "true");
    close(&shell, cx);
}

// EY-282: a slow second tap on the grid icon, off the painted caption, ends the slow click instead.
#[gpui::test]
fn a_second_slow_tap_on_a_grid_icon_never_renames(cx: &mut gpui::TestAppContext) {
    let (shell, cx, _) = ready(cx, "default");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_view(crate::window::startup::View::Grid, window, cx)
        })
    });
    draw(cx);
    let other = read(&shell, cx, "rowNameCentre", &["1"]);
    let caption = read(&shell, cx, "rowNameCentre", &["2"]);
    let slot: Vec<f32> = read(&shell, cx, "rowThumbRect", &["2"])
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    let icon = format!("{} {}", slot[0] + slot[2] / 2., slot[1] + slot[3] / 2.);
    press(cx, &other, 1);
    wait(cx, 600);
    assert_eq!(read(&shell, cx, "cursor", &[]), "1");
    press(cx, &icon, 1);
    wait(cx, 600);
    assert_eq!(
        read(&shell, cx, "cursor", &[]),
        "2",
        "the icon press lands on row 2"
    );
    press(cx, &caption, 1);
    wait(cx, 600);
    assert_eq!(
        editing(&shell, cx),
        None,
        "the caption tap only arms the slow click"
    );
    press(cx, &icon, 1);
    wait(cx, 500);
    draw(cx);
    assert_eq!(
        editing(&shell, cx),
        None,
        "a slow tap on the icon only selects"
    );
    close(&shell, cx);
}
