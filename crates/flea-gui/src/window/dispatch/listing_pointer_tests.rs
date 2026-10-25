//! A pointer gesture that sets the marks on the landed row spends the landing, so Escape clears instead of climbing.
use super::test_shell::{asked, land, open_with, row, set_cursor};
use crate::state::Shell;
use flea::protocol::RowFacts;
use gpui::{
    point, px, Entity, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, TestAppContext, VisualTestContext,
};

pub(super) fn selected(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<usize> {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().selected.clone()
    })
}

// Lands on /files from /files/beta, the last of two rows, so beta is the landed row.
fn landed_on_beta(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    landed_on_beta_in(cx, vec![row("alpha", true), row("beta", true)])
}

// The same landing on rows of the caller's own, beta still at row 1.
pub(super) fn landed_on_beta_in(
    cx: &mut TestAppContext,
    rows: Vec<RowFacts>,
) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_with(
        cx,
        r#""keys":"default","view":"list","escapeUp":true"#,
        None,
    );
    land(&shell, cx, "/files", rows.clone(), 1);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("enter");
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 2);
    cx.simulate_keystrokes("escape");
    land(&shell, cx, "/files", rows, 3);
    assert_eq!(
        selected(&shell, cx),
        vec![1],
        "the folder left is selected on landing"
    );
    (shell, cx)
}

// The middle of `row`, down the list's left edge.
pub(super) fn row_point(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    row: usize,
) -> Point<Pixels> {
    shell.read_with(cx, |shell, _| {
        let input = &shell.panes[0].as_ref().unwrap().input;
        let bounds = input.frame.expect("the list is drawn").bounds;
        let at = |dy: i32| point(bounds.left() + px(40.), bounds.top() + px(dy as f32));
        let rows: Vec<i32> = (0..f32::from(bounds.size.height) as i32)
            .filter(|dy| input.hit(at(*dy)) == Some(row))
            .collect();
        at((rows.first().expect("the row is drawn") + rows.last().unwrap()) / 2)
    })
}

// One press and release of `button` at `at`, as the window delivers them.
pub(super) fn click(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    at: Point<Pixels>,
    button: MouseButton,
    click_count: usize,
    modifiers: Modifiers,
) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let down = MouseDownEvent {
                position: at,
                button,
                click_count,
                modifiers,
                first_mouse: false,
            };
            shell.pointer_down(0, &down, window, cx);
            let up = MouseUpEvent {
                position: at,
                button,
                click_count,
                modifiers,
            };
            shell.pointer_up(0, &up, window, cx);
        })
    });
    cx.run_until_parked();
}

// The middle of row 1, and a point in the empty space below it and to its right, so a band has width.
fn beta_and_below(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
) -> (Point<Pixels>, Point<Pixels>) {
    shell.read_with(cx, |shell, _| {
        let input = &shell.panes[0].as_ref().unwrap().input;
        let bounds = input.frame.expect("the list is drawn").bounds;
        let at = |dy: i32| point(bounds.left() + px(40.), bounds.top() + px(dy as f32));
        let height = f32::from(bounds.size.height) as i32;
        let rows: Vec<i32> = (0..height)
            .filter(|dy| input.hit(at(*dy)) == Some(1))
            .collect();
        let (first, last) = (
            *rows.first().expect("row 1 is drawn"),
            *rows.last().unwrap(),
        );
        let below = (last + 1..height)
            .map(at)
            .find(|point| input.hit(*point).is_none())
            .expect("the list leaves empty space below its rows");
        (
            at((first + last) / 2),
            point(below.x + px(30.), below.y + px(20.)),
        )
    })
}

fn escape_keeps_home(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    assert_eq!(selected(shell, cx), vec![1]);
    cx.simulate_keystrokes("escape");
    assert_eq!(
        selected(shell, cx),
        Vec::<usize>::new(),
        "Escape clears the mark"
    );
    assert_eq!(asked(shell, cx), "/files", "and stays in the folder");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_click_on_the_landed_row_spends_the_landing(cx: &mut TestAppContext) {
    let (shell, cx) = landed_on_beta(cx);
    let (at, _) = beta_and_below(&shell, cx);
    click(&shell, cx, at, MouseButton::Left, 1, Modifiers::none());
    escape_keeps_home(&shell, cx);
}

#[gpui::test]
fn a_band_that_marks_the_landed_row_spends_the_landing(cx: &mut TestAppContext) {
    let (shell, cx) = landed_on_beta(cx);
    let (beta, start) = beta_and_below(&shell, cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let down = MouseDownEvent {
                position: start,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            };
            shell.pointer_down(0, &down, window, cx);
            let drag = MouseMoveEvent {
                position: beta,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            };
            shell.pointer_move(0, &drag, true, window, cx);
            let up = MouseUpEvent {
                position: beta,
                button: MouseButton::Left,
                click_count: 1,
                modifiers: Modifiers::none(),
            };
            shell.pointer_up(0, &up, window, cx);
        })
    });
    cx.run_until_parked();
    escape_keeps_home(&shell, cx);
}
