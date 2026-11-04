//! Grid and Columns pointer and Return parity through the mounted native routes.
use super::{
    pb_grid_tests::{at, switch, tap},
    rename_tests::{close, draw, editing, ready, sent, Lines},
    test_shell::{asked, land, open, open_with, row, set_cursor},
};
use crate::{backend::Backend, state::Shell, window::startup::View};
use gpui::{point, px, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use std::{cell::RefCell, time::Duration};

// Open Mode is read at launch, so the single-click tests seed it in their launch settings.
const SINGLE: &str = r#""keys":"default","view":"list","openMode":"single""#;

fn ready_single(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, Lines) {
    let (shell, cx) = open_with(cx, SINGLE, None);
    land(
        &shell,
        cx,
        "/files",
        vec![
            row("alpha", true),
            row("beta", true),
            row("notes.txt", false),
        ],
        1,
    );
    set_cursor(&shell, cx, 2);
    let (backend, lines) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    (shell, cx, Box::new(lines))
}

// Past the slow-click interval, so an armed rename would have fired.
const PAST_SLOW: Duration = Duration::from_millis(crate::input::slow::DOUBLE_CLICK_MS + 200);

#[gpui::test]
fn pb_grid_icon_and_drag_releases_never_arm_slow_rename(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = ready(cx, "default");
    switch(&shell, cx, View::Grid);
    let icon = at(&shell, cx, 2, false);
    for _ in 0..2 {
        tap(cx, icon, 1, Modifiers::none());
        cx.executor().advance_clock(PAST_SLOW);
        draw(cx);
        assert_eq!(editing(&shell, cx), None, "an icon tap never renames");
    }
    let name = at(&shell, cx, 2, true);
    tap(cx, name, 1, Modifiers::none());
    cx.executor().advance_clock(PAST_SLOW);
    draw(cx);
    assert_eq!(
        editing(&shell, cx),
        None,
        "the first caption tap only records"
    );
    cx.simulate_mouse_down(name, MouseButton::Left, Modifiers::none());
    let away = name + point(px(20.), px(0.));
    cx.simulate_mouse_move(away, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_move(name, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(name, MouseButton::Left, Modifiers::none());
    cx.executor().advance_clock(PAST_SLOW);
    draw(cx);
    assert_eq!(editing(&shell, cx), None, "a drag release never renames");
    // FileDrag.qml liftBegan cancels the slow click, so the next caption tap only records again.
    tap(cx, name, 1, Modifiers::none());
    cx.executor().advance_clock(PAST_SLOW);
    draw(cx);
    assert_eq!(editing(&shell, cx), None, "the lift ended the slow click");
    tap(cx, name, 1, Modifiers::none());
    cx.executor().advance_clock(PAST_SLOW);
    draw(cx);
    assert_eq!(
        editing(&shell, cx).map(|edit| edit.0),
        Some(2),
        "a plain second caption tap renames, so the slow path above is live"
    );
    close(&shell, cx);
}

thread_local! {
    static OPENED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}
fn record(argv: &[String]) -> crate::g2::op3::lane::Exit {
    OPENED.with(|opened| opened.borrow_mut().push(argv.to_vec()));
    (Some(0), String::new())
}

#[gpui::test]
fn pb_grid_single_click_opens_a_file_once_and_modifiers_only_select(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = ready_single(cx);
    shell.update(cx, |shell, _| shell.opener[0].launch = record);
    OPENED.with(|opened| opened.borrow_mut().clear());
    switch(&shell, cx, View::Grid);
    let name = at(&shell, cx, 2, true);
    tap(
        cx,
        name,
        1,
        Modifiers {
            control: true,
            ..Modifiers::none()
        },
    );
    assert!(OPENED.with(|opened| opened.borrow().is_empty()));
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().selected,
            [2],
            "a modified tap selects the file"
        )
    });
    tap(cx, name, 1, Modifiers::none());
    tap(cx, name, 2, Modifiers::none());
    draw(cx);
    let opened = OPENED.with(|opened| opened.borrow().clone());
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0][1..], ["--open", "/files/notes.txt"]);
    close(&shell, cx);
}

#[gpui::test]
fn pb_grid_single_click_opens_a_folder_on_the_first_tap(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready_single(cx);
    switch(&shell, cx, View::Grid);
    sent(&mut lines);
    let icon = at(&shell, cx, 0, false);
    tap(cx, icon, 1, Modifiers::none());
    assert_eq!(asked(&shell, cx), "/files/alpha");
    assert!(sent(&mut lines)
        .iter()
        .any(|line| line.contains(r#""path":"/files/alpha""#)));
    close(&shell, cx);
}

#[gpui::test]
fn pb_grid_columns_middle_click_opens_the_pointed_folder_in_a_tab(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("alpha", true), row("notes.txt", false)],
        1,
    );
    switch(&shell, cx, View::Columns);
    let point = shell.read_with(cx, |shell, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let paint = columns
            .painters
            .get(crate::columns::geometry::Slot::Active)
            .unwrap()
            .borrow();
        let drawn = paint.drawn.as_ref().unwrap();
        drawn.bounds.origin + point(px(40.), px(crate::list::pane::ROW_HEIGHT / 2.))
    });
    let before = shell.read_with(cx, |shell, _| {
        shell.strip.tabs[0]
            .tabs
            .as_ref()
            .map_or(1, |tabs| tabs.items.len())
    });
    cx.simulate_mouse_move(point, None, Modifiers::none());
    cx.simulate_mouse_down(point, MouseButton::Middle, Modifiers::none());
    cx.simulate_mouse_up(point, MouseButton::Middle, Modifiers::none());
    draw(cx);
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.strip.tabs[0]
                .tabs
                .as_ref()
                .map_or(1, |tabs| tabs.items.len()),
            before + 1
        )
    });
    assert_eq!(asked(&shell, cx), "/files/alpha");
    close(&shell, cx);
}

#[gpui::test]
fn pb_grid_columns_single_click_opens_the_active_folder_on_the_first_tap(cx: &mut TestAppContext) {
    let (shell, cx) = open_with(cx, SINGLE, None);
    land(
        &shell,
        cx,
        "/files",
        vec![row("alpha", true), row("notes.txt", false)],
        1,
    );
    switch(&shell, cx, View::Columns);
    let point = shell.read_with(cx, |shell, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let paint = columns
            .painters
            .get(crate::columns::geometry::Slot::Active)
            .unwrap()
            .borrow();
        let drawn = paint.drawn.as_ref().unwrap();
        drawn.bounds.origin + point(px(40.), px(crate::list::pane::ROW_HEIGHT / 2.))
    });
    super::pb_grid_tests::tap(cx, point, 1, Modifiers::none());
    assert_eq!(asked(&shell, cx), "/files/alpha");
    close(&shell, cx);
}

// Grid Enter: Return opens the Grid cursor's row, a folder in the pane and a file through the opener once.
#[gpui::test]
fn pb_grid_return_opens_the_cursor_row(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    shell.update(cx, |shell, _| shell.opener[0].launch = record);
    OPENED.with(|opened| opened.borrow_mut().clear());
    land(
        &shell,
        cx,
        "/files",
        vec![row("alpha", true), row("notes.txt", false)],
        1,
    );
    switch(&shell, cx, View::Grid);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("enter");
    draw(cx);
    let opened = OPENED.with(|opened| opened.borrow().clone());
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0][1..], ["--open", "/files/notes.txt"]);
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("enter");
    assert_eq!(asked(&shell, cx), "/files/alpha");
    close(&shell, cx);
}
