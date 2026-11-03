//! Mounted parity regressions for Grid caption marks, with the tap helpers the Grid pointer tests share.
use super::rename_tests::{close, draw, ready};
use crate::{state::Shell, window::startup::View};
use gpui::{
    point, px, Entity, Modifiers, MouseButton, MouseDownEvent, Pixels, Point, TestAppContext,
    VisualTestContext,
};
use std::time::Duration;

pub(super) fn switch(shell: &Entity<Shell>, cx: &mut VisualTestContext, view: View) {
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(view, window, cx)));
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(400));
        draw(cx);
    }
}

pub(super) fn at(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    row: usize,
    name: bool,
) -> Point<Pixels> {
    shell.read_with(cx, |shell, _| {
        let state = shell.panes[0]
            .as_ref()
            .unwrap()
            .grid
            .as_ref()
            .unwrap()
            .state
            .borrow();
        let tile = state.drawn_row(row).expect("the tile is painted");
        let rect = if name { tile.painted } else { tile.slot };
        state.bounds.origin + point(px(rect.x + rect.width / 2.), px(rect.y + rect.height / 2.))
    })
}

pub(super) fn tap(
    cx: &mut VisualTestContext,
    at: Point<Pixels>,
    count: usize,
    modifiers: Modifiers,
) {
    cx.simulate_mouse_move(at, None, modifiers);
    cx.simulate_event(MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        click_count: count,
        modifiers,
        first_mouse: false,
    });
    cx.simulate_mouse_up(at, MouseButton::Left, modifiers);
    cx.run_until_parked();
}

#[gpui::test]
fn pb_grid_filter_marks_painted_caption_runs_and_clearing_removes_them(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = ready(cx, "default");
    switch(&shell, cx, View::Grid);
    cx.simulate_keystrokes("/");
    cx.simulate_input("a");
    shell.update(cx, |shell, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        let id = host.filter.owner.asked().expect("typing requests a filter").id;
        let listing = host.pane.held.listing;
        assert!(shell.filter_line(0, &format!(
            r#"{{"t":"filtered","id":{id},"query":"a","n":3,"rows":[0,1],"ms":0.01,"listing":{listing}}}"#
        ), cx));
    });
    draw(cx);
    shell.read_with(cx, |shell, _| {
        let state = shell.panes[0]
            .as_ref()
            .unwrap()
            .grid
            .as_ref()
            .unwrap()
            .state
            .borrow();
        assert!(
            state
                .drawn_row(1)
                .unwrap()
                .washes
                .iter()
                .any(Option::is_some),
            "a noncursor caption paints its matching run"
        );
        assert!(
            state
                .drawn_row(0)
                .unwrap()
                .washes
                .iter()
                .all(Option::is_none),
            "the cursor uses bold text instead of an inner box"
        );
    });
    cx.simulate_keystrokes("escape escape");
    draw(cx);
    shell.read_with(cx, |shell, _| {
        let state = shell.panes[0]
            .as_ref()
            .unwrap()
            .grid
            .as_ref()
            .unwrap()
            .state
            .borrow();
        assert!(state
            .drawn()
            .iter()
            .all(|tile| tile.washes.iter().all(Option::is_none)));
    });
    close(&shell, cx);
}
