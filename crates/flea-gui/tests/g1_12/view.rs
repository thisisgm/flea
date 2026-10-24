//! The path surface in a window: the strip's first-tap parent, the field's keys, Tab's peek and both ways to choose a row.
use super::{
    inputs,
    jump::{answer, Folders},
    HOME,
};
use flea::i18n::I18n;
use flea_gui::path::{
    paint::Size,
    peek_line,
    render::pane_strip,
    suggestions::{Ranker, Unranked},
    view::{Inputs, Out, PathView},
    Answer, Peeked,
};
use gpui::{
    div, point, prelude::*, px, size, Context, Entity, Modifiers, MouseButton, MouseDownEvent,
    ScrollDelta, ScrollWheelEvent, Subscription, TestAppContext, VisualTestContext, Window,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// The composition in small: the chrome's 26 px path slot over two 27 px pane strips.
pub struct Host {
    pub path: Entity<PathView>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(0.))
                    .w(px(788.))
                    .h(px(26.))
                    .child(self.path.clone()),
            )
            .child(
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(26.))
                    .w(px(400.))
                    .h(px(27.))
                    .child(pane_strip(&self.path, 0, cx)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(401.))
                    .top(px(26.))
                    .w(px(387.))
                    .h(px(27.))
                    .child(pane_strip(&self.path, 1, cx)),
            )
    }
}

pub struct Bar<'a> {
    pub view: Entity<PathView>,
    pub cx: &'a mut VisualTestContext,
    pub said: Rc<RefCell<Vec<Out>>>,
    pub _events: Subscription,
}

impl Bar<'_> {
    pub fn sync(&mut self, inputs: Inputs) {
        self.view.update(self.cx, |view, cx| view.sync(inputs, cx));
        self.cx.run_until_parked();
    }

    pub fn advance(&mut self, size: Size) -> f32 {
        self.view.read_with(self.cx, |view, _| {
            f32::from(view.paint.borrow().measured(size))
        })
    }

    pub fn press(&mut self, x: f32, y: f32, taps: usize) {
        self.cx.simulate_event(MouseDownEvent {
            position: point(px(x), px(y)),
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count: taps,
            first_mouse: false,
        });
        self.cx
            .simulate_mouse_up(point(px(x), px(y)), MouseButton::Left, Modifiers::none());
    }

    pub fn take(&mut self) -> Vec<Out> {
        std::mem::take(&mut *self.said.borrow_mut())
    }

    pub fn update<R>(
        &mut self,
        f: impl FnOnce(&mut PathView, &mut Window, &mut Context<PathView>) -> R,
    ) -> R {
        let view = self.view.clone();
        let result = self
            .cx
            .update(|window, cx| view.update(cx, |view, cx| f(view, window, cx)));
        self.cx.run_until_parked();
        result
    }

    pub fn read<R>(&mut self, f: impl FnOnce(&PathView) -> R) -> R {
        self.view.read_with(self.cx, |view, _| f(view))
    }
}

pub fn bar(cx: &mut TestAppContext, ranker: Box<dyn Ranker>, inputs: Inputs) -> Bar<'_> {
    let (host, cx) = cx.add_window_view(|_, cx| Host {
        path: cx.new(|cx| PathView::new(Arc::new(I18n::default()), ranker, cx)),
    });
    cx.simulate_resize(size(px(1100.), px(360.)));
    let view = host.read_with(cx, |host, _| host.path.clone());
    let said = Rc::new(RefCell::new(Vec::new()));
    let log = said.clone();
    let events = cx.update(|_, cx| {
        cx.subscribe(&view, move |_, out: &Out, _| {
            log.borrow_mut().push(out.clone())
        })
    });
    let mut bar = Bar {
        view,
        cx,
        said,
        _events: events,
    };
    bar.sync(inputs);
    bar
}

fn open(side: usize, path: &str) -> Out {
    Out::Open {
        side,
        path: path.into(),
    }
}

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
            Out::Said(I18n::default().tr("path.refused_uri"))
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
pub fn ranked<'a>(cx: &'a mut TestAppContext, query: &str) -> Bar<'a> {
    let mut bar = bar(cx, Box::new(Folders::default()), inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    let asks = bar.take();
    assert!(
        matches!(
            asks.as_slice(),
            [Out::Send { side: 0, .. }, Out::ReadHistory]
        ),
        "{asks:?}"
    );
    let zoxide = [
        "/fx/v/work/Media",
        "/srv/omarchy",
        "/home/gm/Documents/Notes",
        "/home/gm/Music",
    ];
    bar.update(|view, window, cx| view.receive(Answer::Jumped(answer(1, &zoxide)), window, cx));
    bar.update(|view, _, cx| view.history_read(Vec::new(), cx));
    let whole = bar.take();
    assert!(
        matches!(whole.as_slice(), [Out::Send { side: 0, .. }]),
        "{whole:?}"
    );
    bar.update(|view, window, cx| view.receive(Answer::Jumped(answer(2, &zoxide)), window, cx));
    bar.cx.simulate_input(query);
    bar
}

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
    let s = super::jump::board();
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
