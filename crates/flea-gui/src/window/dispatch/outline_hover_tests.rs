//! A List folder opens in place from its mark, nothing changes under the pointer, and > and < take every marked folder.
use super::test_shell::{asked, land, open_root, row, sent};
use crate::{state::Shell, window::u07_tests::read};
use gpui::{point, px, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use std::time::Duration;

const ROWS: usize = 6;

/// One drawn frame, then the glyph every row reads in it.
fn glyphs(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.executor().advance_clock(Duration::from_millis(16));
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    (0..ROWS)
        .map(|index| read(shell, cx, "rowGlyph", &[&index.to_string()]))
        .collect()
}

fn folders(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (f32, f32, f32) {
    let rows = (0..ROWS).map(|n| row(&format!("F{n}"), true)).collect();
    land(shell, cx, "/files", rows, 1);
    let rect: Vec<f32> = read(shell, cx, "rowRect", &["0"])
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    (rect[0], rect[1], rect[3])
}

fn click(cx: &mut VisualTestContext, x: f32, y: f32) {
    let at = point(px(x), px(y));
    cx.simulate_mouse_move(at, None, Modifiers::default());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
}

fn outlines(lines: &[String]) -> Vec<&String> {
    lines
        .iter()
        .filter(|line| flea::json::field_str(line, "c").as_deref() == Some("outline"))
        .collect()
}

#[gpui::test]
fn tree_mark_nothing_changes_under_a_sweeping_or_pressing_pointer(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let (left, top, height) = folders(&shell, cx);
    let rest = glyphs(&shell, cx);
    let mut y = top + 1.;
    for x in [left + 4., left + 200.] {
        while y < top + 4. * height {
            cx.simulate_mouse_move(point(px(x), px(y)), None, Modifiers::default());
            assert_eq!(
                glyphs(&shell, cx),
                rest,
                "x {x} y {y}: a mark changed under the pointer"
            );
            y += 3.;
        }
        y = top + 1.;
    }
    let at = point(px(left + 200.), px(top + 2.5 * height));
    cx.simulate_mouse_move(at, None, Modifiers::default());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    assert_eq!(glyphs(&shell, cx), rest, "a press changed a mark");
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    for _ in 0..3 {
        assert_eq!(glyphs(&shell, cx), rest, "a release changed a mark");
    }
}

#[gpui::test]
fn tree_mark_a_click_on_the_mark_opens_and_a_click_on_the_name_selects(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let (left, top, height) = folders(&shell, cx);
    let row2 = top + 2.5 * height;
    sent();
    // The target runs from the row's edge to the gap before the name, 37px at the first level.
    click(cx, left + 1., row2);
    let lines = sent();
    assert_eq!(
        outlines(&lines),
        [r#"{"c":"outline","rows":[2],"open":true,"anchor":0,"listing":1,"first":320}"#],
        "{lines:?}"
    );
    assert_eq!(
        read(&shell, cx, "cursor", &[]),
        "0",
        "a mark click leaves the cursor where it was"
    );
    let mut rows: Vec<_> = (0..ROWS).map(|n| row(&format!("F{n}"), true)).collect();
    rows[2].open = true;
    let mut child = row("F2/inner", false);
    child.depth = 1;
    rows.insert(3, child);
    land(&shell, cx, "/files", rows, 2);
    assert_eq!(read(&shell, cx, "rowGlyph", &["2"]), "folder-open");
    assert_eq!(read(&shell, cx, "cursor", &[]), "0");
    click(cx, left + 36., row2);
    assert!(
        sent()
            .iter()
            .any(|line| line.contains(r#""rows":[2],"open":false"#)),
        "a second click on the open mark closes it"
    );
    // Just past the target, in the gap before the name, a click selects as before.
    land(
        &shell,
        cx,
        "/files",
        (0..ROWS).map(|n| row(&format!("F{n}"), true)).collect(),
        3,
    );
    click(cx, left + 38., top + 4.5 * height);
    assert!(
        outlines(&sent()).is_empty(),
        "the name's side never opens in place"
    );
    assert_eq!(read(&shell, cx, "cursor", &[]), "4");
}

#[gpui::test]
fn tree_mark_a_double_click_on_the_name_still_goes_in(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let (left, top, height) = folders(&shell, cx);
    let at = point(px(left + 200.), px(top + 1.5 * height));
    sent();
    cx.simulate_mouse_move(at, None, Modifiers::default());
    cx.simulate_click(at, Modifiers::default());
    cx.simulate_event(gpui::MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui::MouseUpEvent {
        position: at,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    cx.run_until_parked();
    assert_eq!(asked(&shell, cx), "/files/F1");
    assert!(outlines(&sent()).is_empty());
}

#[gpui::test]
fn tree_mark_greater_and_less_act_on_every_marked_folder(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let mut rows: Vec<_> = (0..ROWS).map(|n| row(&format!("F{n}"), true)).collect();
    rows[4] = row("note.txt", false);
    land(&shell, cx, "/files", rows.clone(), 1);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![1, 3, 4, 900];
        host.pane.cursor = 5;
    });
    sent();
    cx.simulate_keystrokes(">");
    cx.run_until_parked();
    let lines = sent();
    assert_eq!(
        outlines(&lines),
        [r#"{"c":"outline","rows":[1,3,900],"open":true,"anchor":5,"listing":1,"first":320}"#],
        "every marked folder, no marked file, and a marked row outside the held window: {lines:?}"
    );
    rows[1].open = true;
    rows[3].open = true;
    land(&shell, cx, "/files", rows, 2);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![1, 3];
        host.pane.cursor = 0;
    });
    cx.simulate_keystrokes("<");
    cx.run_until_parked();
    assert!(sent()
        .iter()
        .any(|line| line.contains(r#""rows":[1,3],"open":false,"anchor":0"#)));
    // With no marked folder, Ctrl+Right keeps acting on the cursor's folder.
    land(
        &shell,
        cx,
        "/files",
        (0..ROWS).map(|n| row(&format!("F{n}"), true)).collect(),
        3,
    );
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected.clear();
        host.pane.cursor = 2;
    });
    cx.simulate_keystrokes("ctrl-right");
    cx.run_until_parked();
    assert!(sent()
        .iter()
        .any(|line| line.contains(r#""rows":[2],"open":true,"anchor":2"#)));
}

#[gpui::test]
fn tree_mark_marked_folders_alone_decide_and_an_unseen_mark_reaches_the_backend(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_root(cx, "default");
    let mut rows: Vec<_> = (0..ROWS).map(|n| row(&format!("F{n}"), true)).collect();
    rows[2].open = true;
    rows[4] = row("note.txt", false);
    land(&shell, cx, "/files", rows, 1);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![1, 3];
        host.pane.cursor = 2;
    });
    sent();
    cx.simulate_keystrokes("<");
    cx.run_until_parked();
    assert!(
        outlines(&sent()).is_empty(),
        "marked folders already closed leave the cursor's open folder alone"
    );
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![4, 900];
        host.pane.cursor = 3;
    });
    cx.simulate_keystrokes(">");
    cx.run_until_parked();
    let lines = sent();
    assert_eq!(
        outlines(&lines),
        [r#"{"c":"outline","rows":[900],"open":true,"anchor":3,"listing":1,"first":320}"#],
        "a mark outside the held window goes to the backend, which opens the anchor's folder if it is no folder: {lines:?}"
    );
}

#[gpui::test]
fn tree_mark_a_close_names_the_cursor_row_as_its_anchor(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let mut folder = row("A", true);
    folder.open = true;
    let mut child = row("A/inner.txt", false);
    child.depth = 1;
    land(
        &shell,
        cx,
        "/files",
        vec![folder, child, row("end.txt", false)],
        1,
    );
    super::test_shell::set_cursor(&shell, cx, 1);
    let (left, top, height) = {
        let rect: Vec<f32> = read(&shell, cx, "rowRect", &["0"])
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        (rect[0], rect[1], rect[3])
    };
    sent();
    click(cx, left + 10., top + 0.5 * height);
    assert!(
        sent()
            .iter()
            .any(|line| line.contains(r#""rows":[0],"open":false,"anchor":1"#)),
        "the backend moves a cursor hidden by the close; the window names the cursor row"
    );
}
