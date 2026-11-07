//! EY-150: cohesionControls answers over the main window, and focusStop keeps the pane's list stop while Settings holds the keyboard.
use super::dispatch::test_shell::{land, open_root};
use super::u07_tests::read;
use flea::{
    jsondoc::{self, Json},
    protocol::RowFacts,
};

fn controls(state: &Json, kind: &str) -> Vec<Json> {
    state
        .get("controls")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .filter(|control| control.get("kind").and_then(Json::as_str) == Some(kind))
        .cloned()
        .collect()
}

/// The control's centre from its rounded box, within a pixel of what the centre readers round.
fn near(control: &Json, centre: &str) -> bool {
    let n = |key: &str| control.get(key).and_then(Json::as_f64).unwrap();
    let read: Vec<f64> = centre
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    (n("x") + n("w") / 2. - read[0]).abs() <= 1. && (n("y") + n("h") / 2. - read[1]).abs() <= 1.
}

#[gpui::test]
fn fixreaders_ey150_main_window_cohesion_controls_and_focus_stop(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = open_root(cx, "");
    // The list holds the keyboard only in the active window; a TestWindow starts inactive.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let file = |name: &str| RowFacts {
        name: name.into(),
        ..RowFacts::default()
    };
    land(&shell, cx, "/files", vec![file("a"), file("b")], 1);
    let state = jsondoc::parse(&read(&shell, cx, "cohesionControls", &[])).unwrap();
    let buttons = controls(&state, "ChromeButton");
    let search = buttons
        .iter()
        .find(|b| b.get("name").and_then(Json::as_str) == Some("Search"))
        .expect("the toolbar's Search button");
    let drawn = read(&shell, cx, "chromeButtonCentre", &["search"]);
    assert!(near(search, &drawn), "{search:?} {drawn}");
    let back = buttons
        .iter()
        .find(|b| b.get("name").and_then(Json::as_str) == Some("Back"))
        .expect("Back draws");
    assert_eq!(back.get("enabled").and_then(Json::as_bool), Some(false));
    // The list holds the keyboard, so focus is its cursor row, where rowRect reads it.
    let row = read(&shell, cx, "rowRect", &["0"]);
    assert!(!row.is_empty(), "the landed listing draws row 0");
    let focus = state.get("focus").expect("cohesionControls names a focus");
    let n = |key: &str| focus.get(key).and_then(Json::as_f64).unwrap();
    assert_eq!(format!("{} {} {} {}", n("x"), n("y"), n("w"), n("h")), row);
    assert_eq!(focus.get("via").and_then(Json::as_str), Some("item"));
    // Down moves the cursor to row 1, and focus follows it off row 0.
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let next = read(&shell, cx, "rowRect", &["1"]);
    assert_ne!(next, row, "rows 0 and 1 draw apart");
    let moved = jsondoc::parse(&read(&shell, cx, "cohesionControls", &[])).unwrap();
    let focus = moved.get("focus").expect("cohesionControls names a focus");
    let n = |key: &str| focus.get(key).and_then(Json::as_f64).unwrap();
    assert_eq!(format!("{} {} {} {}", n("x"), n("y"), n("w"), n("h")), next);
    assert_eq!(read(&shell, cx, "focusStop", &[]), "list");
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_settings(window, cx)));
    cx.run_until_parked();
    assert_eq!(read(&shell, cx, "settingsOpen", &[]), "true");
    assert_eq!(read(&shell, cx, "focusStop", &[]), "list");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(read(&shell, cx, "settingsOpen", &[]), "false");
    let state = jsondoc::parse(&read(&shell, cx, "cohesionControls", &[])).unwrap();
    assert!(!controls(&state, "ChromeButton").is_empty());
}
