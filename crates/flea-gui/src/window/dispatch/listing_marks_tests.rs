//! EY-33: a landing's row is a lone mark (Nav.js applyPendingSelect only(index, true)), so v keeps it and j carries it.
use super::listing_pointer_tests::selected;
use super::test_shell::{asked, land, open_with, row};
use crate::state::Shell;
use gpui::{Entity, TestAppContext, VisualTestContext};

fn launched_on_beta<'a>(
    cx: &'a mut TestAppContext,
    view: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = open_with(
        cx,
        &format!(r#""keys":"default","view":"{view}","escapeUp":true"#),
        Some("/files/beta"),
    );
    let rows = ["alpha", "beta", "gamma", "delta"]
        .map(|name| row(name, false))
        .to_vec();
    land(&shell, cx, "/files", rows, 1);
    assert_eq!(selected(&shell, cx), vec![1], "the asked row is selected");
    (shell, cx)
}

// Selection.js promote: v on the landed lone row keeps it, then v on the next row marks a second one.
#[gpui::test]
fn v_j_v_after_a_select_launch_keeps_two_marks_in_list(cx: &mut TestAppContext) {
    let (shell, cx) = launched_on_beta(cx, "list");
    cx.simulate_keystrokes("v j v");
    assert_eq!(selected(&shell, cx), vec![1, 2], "both rows stay marked");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn v_j_v_after_a_select_launch_keeps_two_marks_in_columns(cx: &mut TestAppContext) {
    let (shell, cx) = launched_on_beta(cx, "columns");
    cx.simulate_keystrokes("v j v");
    assert_eq!(selected(&shell, cx), vec![1, 2], "both rows stay marked");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// Marks.js follow: a plain move carries the landed lone row, so the cursor row is the selection.
#[gpui::test]
fn a_plain_move_carries_the_landed_row(cx: &mut TestAppContext) {
    let (shell, cx) = launched_on_beta(cx, "list");
    cx.simulate_keystrokes("j");
    assert_eq!(
        selected(&shell, cx),
        vec![2],
        "the lone mark follows the cursor"
    );
    cx.simulate_keystrokes("v v");
    assert_eq!(
        selected(&shell, cx),
        Vec::<usize>::new(),
        "v keeps the lone row, the second v drops it"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// Focus.js escapeUp with Selection.js isLanded: the carried landing is still no mark, so Escape climbs through it.
#[gpui::test]
fn escape_climbs_through_a_carried_landing(cx: &mut TestAppContext) {
    let (shell, cx) = launched_on_beta(cx, "list");
    cx.simulate_keystrokes("j");
    assert_eq!(selected(&shell, cx), vec![2]);
    cx.simulate_keystrokes("escape");
    assert_eq!(
        asked(&shell, cx),
        "/",
        "Escape climbs past the carried landing"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
