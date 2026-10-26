//! EY-33 and EY-53 in Dual: a select launch's lone row survives v, and t opens and draws a tab on the focused pane.
use super::menus_tests::{home, settle};
use super::test_shell::{asked_on, land, land_on, open_with};
use crate::{state::Shell, window::u07_tests::read};
use gpui::{Entity, TestAppContext, VisualTestContext};

// The sweep's key.dual rows: a Dual launch selecting notes.txt, both panes listed.
fn dual(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_with(
        cx,
        r#""keys":"default","view":"dual""#,
        Some("/files/notes.txt"),
    );
    land(&shell, cx, "/files", home(), 1);
    let right = asked_on(&shell, cx, 1);
    land_on(&shell, cx, 1, &right, home(), 4, 1);
    settle(cx);
    assert_eq!(marks(&shell, cx), (0, vec![2]), "the asked row is selected");
    (shell, cx)
}

fn marks(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (usize, Vec<usize>) {
    shell.read_with(cx, |shell, _| {
        let side = shell.focus_side;
        (side, shell.panes[side].as_ref().unwrap().selected.clone())
    })
}

// Selection.js promote: v on the landed lone row keeps it, so v j v marks two rows.
#[gpui::test]
fn v_keeps_the_landed_row_in_dual(cx: &mut TestAppContext) {
    let (shell, cx) = dual(cx);
    cx.simulate_keystrokes("v");
    assert_eq!(marks(&shell, cx), (0, vec![2]), "v keeps the landed row");
    cx.simulate_keystrokes("j v");
    assert_eq!(marks(&shell, cx), (0, vec![2, 3]), "v j v keeps two marks");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// TabBar.qml open is tabCount > 1 in every view, Dual included.
#[gpui::test]
fn t_opens_and_draws_a_tab_in_dual(cx: &mut TestAppContext) {
    let (shell, cx) = dual(cx);
    assert_eq!(read(&shell, cx, "tabCount", &[]), "1");
    assert_eq!(read(&shell, cx, "tabBarVisible", &[]), "false");
    cx.simulate_keystrokes("t");
    settle(cx);
    assert_eq!(
        read(&shell, cx, "tabCount", &[]),
        "2",
        "t opens a second tab"
    );
    assert_eq!(
        read(&shell, cx, "tabBarVisible", &[]),
        "true",
        "the strip draws over both panes"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// The strip belongs to the focused pane, so a click on a tab drawn over the other pane selects it.
#[gpui::test]
fn a_click_on_a_tab_over_the_unfocused_pane_selects_it(cx: &mut TestAppContext) {
    use crate::tabs::bar::{HEIGHT, PAD_X};
    use gpui::{point, px, Modifiers, MouseButton};
    // Clear of the divider, so the press lands on the right pane's half.
    let past_divider = 10.;
    let (shell, cx) = dual(cx);
    cx.simulate_keystrokes("t t t t t t t t");
    settle(cx);
    assert_eq!(read(&shell, cx, "tabCount", &[]), "9");
    let (start, index) = cx.update(|window, cx| {
        let shell = shell.read(cx);
        let right = shell.geometry(window).panes[1].x;
        let bounds = shell.strip.bounds.get().unwrap();
        let width = shell.strip.snapshot.as_ref().unwrap().frame.tab_width;
        let centre = |i: usize| f32::from(bounds.origin.x) + PAD_X + (i as f32 + 0.5) * width;
        let index = (0..9)
            .find(|&i| centre(i) > right + past_divider)
            .expect("a tab is drawn over the right pane");
        assert!(index < 8, "the tab is not the current one");
        (
            point(px(centre(index)), bounds.origin.y + px(HEIGHT / 2.)),
            index,
        )
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::default());
    settle(cx);
    assert_eq!(marks(&shell, cx).0, 0, "the left pane keeps the focus");
    assert_eq!(read(&shell, cx, "tabIndex", &[]), index.to_string());
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
