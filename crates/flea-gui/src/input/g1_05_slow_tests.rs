use super::*;
use crate::input::slow;

fn named(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) {
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

fn on_name(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) -> gpui::Point<gpui::Pixels> {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let frame = host.input.frame.unwrap();
        let list = host.list.borrow();
        let x = list.drawn.bounds.origin.x + px(list.drawn.layout.name.x + 4.);
        assert!(list.on_name(2, x), "the probe point sits on the drawn name");
        assert!(!list.on_name(
            2,
            list.drawn.bounds.origin.x
                + px(list.drawn.layout.name.x + list.drawn.layout.name.width - 1.)
        ));
        point(x, frame.bounds.top() + px(frame.row * 2.5))
    })
}

fn tap(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
}

#[gpui::test]
fn g1_05_a_slow_second_tap_on_the_sole_marked_name_arms_the_rename(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    named(&shell, cx);
    let at = on_name(&shell, cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    tap(cx, at);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.selected, [2]);
        assert!(
            shell
                .rename
                .slow
                .as_ref()
                .is_none_or(|slow| !slow.pending()),
            "the first tap only selects"
        );
    });
    cx.executor().advance_clock(Duration::from_millis(500));
    tap(cx, at);
    shell.read_with(cx, |shell, _| {
        assert!(
            shell
                .rename
                .slow
                .as_ref()
                .is_some_and(|slow| slow.pending()),
            "a slow second tap arms the timer"
        );
        assert_eq!(
            shell.rename.slow.as_ref().and_then(|slow| slow.row()),
            Some(2)
        );
    });
    cx.executor()
        .advance_clock(Duration::from_millis(slow::DOUBLE_CLICK_MS + 10));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert!(shell
            .rename
            .slow
            .as_ref()
            .is_none_or(|slow| !slow.pending()));
        assert!(
            shell.rename.slow.is_none(),
            "the timer fired and cleared its record"
        );
        assert_eq!(
            shell
                .rename
                .edit
                .as_ref()
                .map(|edit| (edit.index, edit.name.as_str())),
            Some((2, "file-002.txt")),
            "the timer opens the rename editor for the armed row"
        );
    });
}

#[gpui::test]
fn g1_05_a_quick_second_tap_or_a_tap_off_the_name_never_arms(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    named(&shell, cx);
    let at = on_name(&shell, cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    tap(cx, at);
    cx.executor().advance_clock(Duration::from_millis(200));
    tap(cx, at);
    shell.read_with(cx, |shell, _| {
        assert!(
            shell
                .rename
                .slow
                .as_ref()
                .is_none_or(|slow| !slow.pending()),
            "inside the interval is the double click"
        );
    });
    let off = shell.read_with(cx, |shell, _| {
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        point(
            list.drawn.bounds.origin.x
                + px(list.drawn.layout.name.x + list.drawn.layout.name.width - 1.),
            at.y,
        )
    });
    cx.executor().advance_clock(Duration::from_millis(500));
    tap(cx, off);
    cx.executor().advance_clock(Duration::from_millis(500));
    tap(cx, off);
    shell.read_with(cx, |shell, _| {
        assert!(
            shell
                .rename
                .slow
                .as_ref()
                .is_none_or(|slow| !slow.pending()),
            "the empty rest of the name slot is not the name"
        );
        assert!(shell.rename.slow.is_none());
    });
}
