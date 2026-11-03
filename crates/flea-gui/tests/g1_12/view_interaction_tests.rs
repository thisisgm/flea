//! Path strip navigation, editing and suggestion interaction.
use super::*;

#[gpui::test]
fn a_parent_crumb_opens_on_the_press_and_a_rapid_second_tap_with_a_slow_backend_does_nothing(
    cx: &mut TestAppContext,
) {
    let mut bar = bar(cx, Box::new(Unranked), inputs("/home/gm/Work/claude"));
    let cell = bar.advance(Size::Caption);
    assert!(cell > 0.);
    // "~/" "Work/" "claude": the second crumb's middle.
    bar.press(cell * 4.5, 13., 1);
    assert_eq!(
        bar.take(),
        [open(0, "/home/gm/Work")],
        "no double-click wait"
    );
    bar.press(cell * 4.5, 13., 2);
    assert_eq!(
        bar.take(),
        [],
        "the backend has not answered, and the second tap is not another open"
    );
    bar.sync(inputs("/home/gm/Work"));
    assert_eq!(bar.take(), []);
    assert_eq!(bar.read(|view| view.strips[0].crumbs().len()), 2);
}

#[gpui::test]
fn a_double_tap_on_the_leaf_or_the_marker_edits_and_never_navigates(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Unranked), inputs("/home/gm/Work/claude"));
    let cell = bar.advance(Size::Caption);
    bar.press(cell * 9., 13., 1);
    bar.press(cell * 9., 13., 2);
    assert_eq!(bar.take(), []);
    assert!(bar.read(|view| view.editing()));
    assert_eq!(
        bar.read(|view| view.entry.text.clone()),
        "/home/gm/Work/claude/"
    );
    let view = bar.view.clone();
    assert!(
        bar.cx
            .update(|window, cx| view.read(cx).holds_keyboard(window)),
        "the field holds the keyboard"
    );
    bar.cx.simulate_keystrokes("escape");
    assert_eq!(bar.take(), [Out::Closed]);
    // A path too long for 788 px collapses, and the marker is inert too.
    let deep = format!("{HOME}/{}/x/y/z", "a".repeat(120));
    bar.sync(inputs(&deep));
    assert!(bar.read(|view| view.strips[0].crumbs()[1].elided));
    bar.press(cell * 2.5, 13., 1);
    assert_eq!(bar.take(), []);
    bar.press(cell * 2.5, 13., 2);
    assert!(bar.read(|view| view.editing()));
    assert_eq!(bar.take(), []);
}

#[gpui::test]
fn typing_a_path_and_enter_opens_it_and_escape_opens_nothing(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Unranked), inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    assert_eq!(
        bar.take(),
        [],
        "an unranked bar spends no backend ask on open"
    );
    bar.cx.simulate_input("/etc/apt");
    assert_eq!(
        bar.read(|view| view.entry.text.clone()),
        "/etc/apt",
        "the typed line replaces the selected prefill"
    );
    bar.cx
        .simulate_keystrokes("backspace backspace backspace backspace");
    bar.cx.simulate_keystrokes("enter");
    assert_eq!(bar.take(), [Out::Closed, open(0, "/etc")]);
    bar.update(|view, window, cx| view.open(window, cx));
    bar.cx.simulate_input("~/Downloads");
    bar.cx.simulate_keystrokes("escape");
    assert_eq!(bar.take(), [Out::Closed]);
    assert!(!bar.read(|view| view.editing()));
}

#[gpui::test]
fn the_window_binds_the_retry_and_the_bar_gates_the_same_path_on_it(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Unranked), inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    bar.cx.simulate_keystrokes("enter");
    assert_eq!(
        bar.take(),
        [Out::Closed],
        "pathbar.js:146, a settled same path is a no-op"
    );
    bar.sync(Inputs {
        failed: true,
        ..inputs(HOME)
    });
    bar.update(|view, window, cx| view.open(window, cx));
    bar.cx.simulate_keystrokes("enter");
    assert_eq!(
        bar.take(),
        [Out::Closed, open(0, HOME)],
        "pathbar.js:144, the failed listing retries"
    );
    bar.update(|view, window, cx| view.open(window, cx));
    bar.cx.simulate_input("smb://server.lan");
    bar.cx.simulate_keystrokes("enter");
    assert_eq!(
        bar.take(),
        [Out::Closed, Out::Network("smb://server.lan".into())]
    );
    bar.update(|view, window, cx| view.open(window, cx));
    bar.cx.simulate_input("file://otherbox/etc");
    bar.cx.simulate_keystrokes("enter");
    assert_eq!(
        bar.take(),
        [
            Out::Closed,
            Out::Said(crate::test_catalogs::english().tr("path.refused_uri"))
        ]
    );
}

#[gpui::test]
fn tab_asks_the_focused_backend_and_its_peek_completes_the_line(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Unranked), inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    bar.cx.simulate_input("/et");
    bar.cx.simulate_keystrokes("tab");
    assert_eq!(
        bar.take(),
        [Out::Send {
            side: 0,
            line: peek_line("/", 40, false)
        }]
    );
    let peeked = Peeked {
        path: "/".into(),
        dirs: vec!["etc".into(), "home".into()],
        ..Peeked::default()
    };
    bar.update(|view, window, cx| view.receive(Answer::Peeked(peeked), window, cx));
    assert_eq!(bar.read(|view| view.entry.text.clone()), "/etc/");
    bar.cx.simulate_input("zz");
    bar.cx.simulate_keystrokes("tab");
    let line = peek_line("/etc", 40, false);
    assert_eq!(bar.take(), [Out::Send { side: 0, line }]);
    let empty = Peeked {
        path: "/etc".into(),
        dirs: vec!["apt".into()],
        ..Peeked::default()
    };
    bar.update(|view, window, cx| view.receive(Answer::Peeked(empty), window, cx));
    assert_eq!(
        bar.take(),
        [Out::Said("Nothing in /etc starts with that".into())]
    );
}

// Opens the bar with the test ranker, answers both asks and types the query.
#[gpui::test]
fn a_suggestion_is_chosen_by_the_keyboard(cx: &mut TestAppContext) {
    let mut bar = ranked(cx, "o");
    let rows: Vec<String> = bar.read(|view| {
        view.jump
            .entries
            .iter()
            .map(|row| row.path.clone())
            .collect()
    });
    assert_eq!(
        rows,
        [
            "/fx/v/work/Documents",
            "/srv/omarchy",
            "/home/gm/Documents/Notes"
        ]
    );
    assert_eq!(bar.read(|view| view.jump.cursor), Some(0));
    bar.cx.simulate_keystrokes("down down down up");
    assert_eq!(bar.read(|view| view.jump.cursor), Some(1));
    bar.cx.simulate_keystrokes("enter");
    assert_eq!(bar.take(), [Out::Closed, open(0, "/srv/omarchy")]);
}

#[gpui::test]
fn a_suggestion_is_chosen_by_the_pointer_and_a_resting_pointer_takes_no_row(
    cx: &mut TestAppContext,
) {
    let mut bar = ranked(cx, "o");
    let popup = bar
        .read(|view| view.paint.borrow().popup)
        .expect("the popup drew");
    let s = crate::jump::board();
    assert_eq!(
        (popup.x, popup.y, popup.width, popup.height),
        (0., 30., 788., 3. * s.row + 12.)
    );
    let row = |index: f32| 30. + 1. + 5. + index * s.row + s.row / 2.;
    bar.cx
        .simulate_mouse_move(point(px(200.), px(row(2.))), None, Modifiers::none());
    assert_eq!(
        bar.read(|view| view.jump.cursor),
        Some(0),
        "rows that appeared under the pointer do not take it"
    );
    assert!(
        bar.cx.update(|window, cx| window.simulate_next_frame(cx)) > 0,
        "the rows settle on the next frame"
    );
    bar.cx
        .simulate_mouse_move(point(px(200.), px(row(1.))), None, Modifiers::none());
    assert_eq!(bar.read(|view| view.jump.cursor), Some(1));
    bar.cx.simulate_event(ScrollWheelEvent {
        position: point(px(200.), px(row(1.))),
        delta: ScrollDelta::Lines(point(0., -1.)),
        ..ScrollWheelEvent::default()
    });
    assert_eq!(
        bar.read(|view| view.jump.cursor),
        Some(2),
        "a notch steps the highlight"
    );
    bar.press(200., row(1.), 1);
    assert_eq!(bar.take(), [Out::Closed, open(0, "/srv/omarchy")]);
    assert!(!bar.read(|view| view.editing()));
}

#[gpui::test]
fn a_press_on_the_popup_padding_closes_and_a_held_enter_opens_the_whole_answers_first_row(
    cx: &mut TestAppContext,
) {
    let mut bar = ranked(cx, "o");
    bar.press(200., 30. + 3., 1);
    assert_eq!(bar.take(), [Out::Closed]);
    let mut bar = bar_with_held_enter(bar);
    bar.update(|view, window, cx| {
        view.receive(Answer::Jumped(answer(4, &["/srv/media"])), window, cx)
    });
    assert_eq!(bar.take(), [Out::Closed, open(0, "/srv/media")]);
}

// A fresh open whose whole answer has not landed: Enter on a name holds, and typing after it changes nothing.
fn bar_with_held_enter(mut bar: Bar<'_>) -> Bar<'_> {
    bar.update(|view, window, cx| view.open(window, cx));
    let asks = bar.take();
    assert!(
        matches!(asks.as_slice(), [Out::Send { side: 0, .. }]),
        "a kept history asks once: {asks:?}"
    );
    bar.cx.simulate_input("med");
    bar.cx.simulate_keystrokes("enter");
    assert!(bar.read(|view| view.jump.enter_waiting));
    bar.cx.simulate_input("xyz");
    assert_eq!(
        bar.read(|view| view.entry.text.clone()),
        "med",
        "a held Enter stands for the line as it was"
    );
    bar
}

#[gpui::test]
fn the_single_view_builds_no_pane_crumbs_and_dual_strips_take_the_press_for_their_pane(
    cx: &mut TestAppContext,
) {
    let mut bar = bar(cx, Box::new(Unranked), inputs("/home/gm/Work/claude"));
    let built = bar.read(|view| view.strips[0].built);
    for _ in 0..5 {
        let show_hidden = !bar.read(|view| view.inputs.show_hidden);
        bar.sync(Inputs {
            show_hidden,
            ..inputs("/home/gm/Work/claude")
        });
    }
    assert_eq!(
        bar.read(|view| (
            view.strips[0].built,
            view.strips[1].built,
            view.strips[2].built
        )),
        (built, 0, 0)
    );
    let dual = Inputs {
        dual: true,
        paths: ["/home/gm/Work".into(), "/srv/www".into()],
        ..inputs(HOME)
    };
    bar.sync(dual.clone());
    assert_eq!(
        bar.read(|view| (view.strips[1].built, view.strips[2].built)),
        (1, 1)
    );
    let cell = bar.advance(Size::Caption);
    // "/" "srv/" "www" on the right pane, which starts 401 px in and 14 px into its strip.
    bar.press(401. + 14. + cell * 3., 26. + 13., 1);
    assert_eq!(bar.take(), [Out::Focus(1), open(1, "/srv")]);
    assert_eq!(bar.read(|view| view.inputs.side), 1);
    bar.press(14. + cell * 0.5, 26. + 13., 1);
    assert_eq!(bar.take(), [Out::Focus(0), open(0, HOME)]);
    bar.sync(inputs("/home/gm/Work/claude"));
    assert_eq!(
        bar.read(|view| view.strips[1].crumbs().len() + view.strips[2].crumbs().len()),
        0
    );
}
