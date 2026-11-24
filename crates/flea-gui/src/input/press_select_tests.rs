use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
};
use gpui::{
    point, px, size, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext,
};

fn mounted(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_resize(size(px(900.), px(600.)));
    cx.run_until_parked();
    (shell, cx)
}

fn row_at(shell: &Entity<Shell>, cx: &mut VisualTestContext, row: f32) -> Point<Pixels> {
    shell.read_with(cx, |shell, _| {
        let frame = shell.panes[0].as_ref().unwrap().input.frame.unwrap();
        point(
            frame.bounds.left() + px(60.),
            frame.bounds.top() + px(frame.row * (row + 0.5)),
        )
    })
}

fn selection(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (Vec<usize>, usize) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.selected.clone(), host.pane.cursor)
    })
}

#[gpui::test]
fn gh506_a_press_on_an_unselected_row_selects_it_before_release(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let at = row_at(&shell, cx, 2.);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(selection(&shell, cx), (vec![2], 2), "the press selects");
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(selection(&shell, cx), (vec![2], 2), "the release keeps it");
    shell.read_with(cx, |shell, _| {
        assert!(
            shell
                .rename
                .slow
                .as_ref()
                .is_none_or(|slow| !slow.pending()),
            "a first tap that selected on press arms no slow-click rename"
        );
    });
}

#[gpui::test]
fn gh506_a_press_inside_the_selection_waits_for_release(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![2, 4];
        host.pane.cursor = 4;
    });
    let at = row_at(&shell, cx, 2.);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        selection(&shell, cx).0,
        vec![2, 4],
        "the press keeps the selection, so it can still be dragged"
    );
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        selection(&shell, cx),
        (vec![2], 2),
        "a release without drag reduces it"
    );
}

#[gpui::test]
fn gh506_modified_presses_still_decide_on_release(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    let at = row_at(&shell, cx, 3.);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::control());
    cx.run_until_parked();
    assert!(
        selection(&shell, cx).0.is_empty(),
        "a Ctrl press waits for its toggle"
    );
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::control());
    cx.run_until_parked();
    assert!(selection(&shell, cx).0.contains(&3));
}
