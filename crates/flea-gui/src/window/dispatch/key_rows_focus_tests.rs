//! EY-61: Shift+Tab walks the Tab ring backward in every view, as v0.3.12's Focus.js next() does: the last drawn chrome control, then the rail, then the list.
use super::key_rows_tests::launched;
use super::menus_tests::settle;
use crate::state::Shell;
use gpui::{Entity, TestAppContext, VisualTestContext};

// The focused pane and the stop that holds the keyboard.
fn stop(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (usize, &'static str) {
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        (shell.focus_side, shell.focus_view(window, cx))
    })
}

// Each keystroke in order, with the stop it must leave the keyboard on.
fn walk(cx: &mut VisualTestContext, shell: &Entity<Shell>, steps: &[(&str, (usize, &str))]) {
    for (keys, expected) in steps {
        cx.simulate_keystrokes(keys);
        settle(cx);
        assert_eq!(stop(shell, cx), *expected, "{keys}");
    }
}

// The index of the chrome control that holds focus.
fn chrome_control(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<usize> {
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        let view = shell.chrome.view.as_ref()?.read(cx);
        view.focus
            .iter()
            .position(|handle| handle.is_focused(window))
    })
}

// The chrome controls drawn and enabled, in drawn order: the ones a Tab can land on.
fn live_chrome_controls(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<usize> {
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        let view = shell.chrome.view.as_ref().unwrap().read(cx);
        let frame = view.frame.borrow();
        let live: Vec<usize> = frame
            .controls
            .iter()
            .enumerate()
            .filter(|(_, control)| control.visible && control.enabled)
            .map(|(index, _)| index)
            .collect();
        live
    })
}

// Non-dual views: one list, so Tab goes list, rail, chrome and back, and Shift+Tab reverses it.
fn single_list_walks_both_ways(cx: &mut TestAppContext, view: &str) {
    let (shell, cx) = launched(cx, view);
    assert_eq!(stop(&shell, cx), (0, "list"), "{view} starts on the list");
    walk(
        cx,
        &shell,
        &[
            ("shift-tab", (0, "chrome")),
            ("shift-tab", (0, "rail")),
            ("shift-tab", (0, "list")),
            ("tab", (0, "rail")),
            ("tab", (0, "chrome")),
            ("tab", (0, "list")),
        ],
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn shift_tab_walks_the_ring_backward_in_list(cx: &mut TestAppContext) {
    single_list_walks_both_ways(cx, "list");
}

#[gpui::test]
fn shift_tab_walks_the_ring_backward_in_grid(cx: &mut TestAppContext) {
    single_list_walks_both_ways(cx, "grid");
}

#[gpui::test]
fn shift_tab_walks_the_ring_backward_in_columns(cx: &mut TestAppContext) {
    single_list_walks_both_ways(cx, "columns");
}

// Dual: the two lists are consecutive stops, the rail and chrome follow the right list, and Shift+Tab reverses each step.
#[gpui::test]
fn shift_tab_walks_the_ring_backward_in_dual(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "dual");
    assert_eq!(
        stop(&shell, cx),
        (0, "list"),
        "dual starts on the left list"
    );
    walk(
        cx,
        &shell,
        &[
            ("shift-tab", (0, "chrome")),
            ("shift-tab", (0, "rail")),
            ("shift-tab", (1, "list")),
            ("shift-tab", (0, "list")),
            ("tab", (1, "list")),
            ("tab", (0, "rail")),
            ("tab", (0, "chrome")),
            ("tab", (0, "list")),
        ],
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// ChromeKeys.enter: Tab lands on the chrome's first live control, Shift+Tab on its last.
#[gpui::test]
fn chrome_entry_lands_on_its_first_or_last_live_control(cx: &mut TestAppContext) {
    let (shell, cx) = launched(cx, "list");
    let live = live_chrome_controls(&shell, cx);
    assert!(!live.is_empty(), "the chrome draws a control to land on");
    cx.simulate_keystrokes("shift-tab");
    settle(cx);
    assert_eq!(
        chrome_control(&shell, cx),
        live.last().copied(),
        "shift-tab enters at the last live control"
    );
    cx.simulate_keystrokes("shift-tab");
    settle(cx);
    cx.simulate_keystrokes("tab");
    settle(cx);
    assert_eq!(
        chrome_control(&shell, cx),
        live.first().copied(),
        "tab enters at the first live control"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
