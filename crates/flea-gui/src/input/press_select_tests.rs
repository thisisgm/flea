use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::{pane::PaneHost, startup::View},
};
use flea::protocol::{Reply, RowFacts};
use gpui::{
    point, px, size, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext,
};
use std::{rc::Rc, time::Duration};

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
    row_in(shell, cx, 0, row)
}

fn row_in(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    side: usize,
    row: f32,
) -> Point<Pixels> {
    shell.read_with(cx, |shell, _| {
        let frame = shell.panes[side].as_ref().unwrap().input.frame.unwrap();
        point(
            frame.bounds.left() + px(60.),
            frame.bounds.top() + px(frame.row * (row + 0.5)),
        )
    })
}

fn selection(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (Vec<usize>, usize) {
    selection_in(shell, cx, 0)
}

fn selection_in(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    side: usize,
) -> (Vec<usize>, usize) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[side].as_ref().unwrap();
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

// The fixture's rows have no names, and the slow click needs a drawn name under the pointer.
fn named(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        let mut rows = host.pane.held.rows.clone();
        for (at, row) in rows.iter_mut().enumerate() {
            row.name = format!("file-{at:03}.txt");
        }
        host.pane.held = Rc::new(crate::list::pane::Held {
            listing: host.pane.held.listing,
            total: host.pane.held.total,
            start: 0,
            rows,
        });
        cx.notify();
    });
    cx.run_until_parked();
}

fn on_name(shell: &Entity<Shell>, cx: &mut VisualTestContext, row: usize) -> Point<Pixels> {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let frame = host.input.frame.unwrap();
        let list = host.list.borrow();
        let x = list.drawn.bounds.origin.x + px(list.drawn.layout.name.x + 4.);
        assert!(
            list.on_name(row, x),
            "the probe point sits on the drawn name"
        );
        point(x, frame.bounds.top() + px(frame.row * (row as f32 + 0.5)))
    })
}

fn tap(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn gh506_a_slow_tap_that_selects_its_row_arms_no_rename(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    named(&shell, cx);
    let at = on_name(&shell, cx, 2);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    tap(cx, at);
    assert_eq!(selection(&shell, cx), (vec![2], 2));
    // The selection moves away from the tapped row, so it is no longer the sole marked row.
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![4];
        host.pane.cursor = 4;
    });
    cx.executor().advance_clock(Duration::from_millis(500));
    tap(cx, at);
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [2]);
        assert!(
            shell
                .rename
                .slow
                .as_ref()
                .is_none_or(|slow| !slow.pending()),
            "the row was not the sole selection before this press, so the slow tap only selects"
        );
    });
}

fn right(shell: &Shell) -> PaneHost {
    let mut host = PaneHost::new("/right".into(), None, 620., &shell.locale);
    host.pane
        .receive(Reply::Listed {
            path: "/right".into(),
            total: 8,
        })
        .unwrap();
    host.pane
        .receive(Reply::Rows {
            start: 0,
            rows: vec![RowFacts::default(); 8],
            listing: 2,
        })
        .unwrap();
    host
}

#[gpui::test]
fn gh506_both_dual_lists_select_on_press(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[1] = Some(right(&shell));
        shell.view = View::Dual;
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_resize(size(px(1200.), px(600.)));
    cx.run_until_parked();
    for (side, row) in [(0, 2), (1, 3)] {
        let at = row_in(&shell, cx, side, row as f32);
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            selection_in(&shell, cx, side),
            (vec![row], row),
            "pane {side} selects on press"
        );
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
    }
}
