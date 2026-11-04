//! The suggestion readers an open bar answers, read from the rows the shipped ranker drew.
use super::{
    inputs,
    rank::board,
    view::{bar, Bar},
    HOME,
};
use flea_gui::{
    ipc::{path::PathAdapter, Adapter},
    path::{
        rank::Jump,
        render::pane_strip,
        suggestions::{Ranker, Unranked},
        view::{Inputs, Out, PathView},
        Answer,
    },
};
use gpui::{div, point, prelude::*, px, size, Context, Entity, Modifiers, TestAppContext, Window};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// The path slot at the chrome offset, so origin-subtracting readers see a non-zero x.
pub struct OffsetHost {
    pub path: Entity<PathView>,
    pub x: f32,
}

impl Render for OffsetHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(
                div()
                    .absolute()
                    .left(px(self.x))
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

pub fn bar_at(cx: &mut TestAppContext, ranker: Box<dyn Ranker>, inputs: Inputs, x: f32) -> Bar<'_> {
    let (host, cx) = cx.add_window_view(|_, cx| OffsetHost {
        path: cx.new(|cx| PathView::new(Arc::new(crate::test_catalogs::english()), ranker, cx)),
        x,
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

fn suggest(cx: &mut TestAppContext, ranker: Box<dyn Ranker>, typed: &str) -> (String, Vec<String>) {
    let mut bar = bar(cx, ranker, inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(board()), window, cx));
    bar.cx.simulate_input(typed);
    bar.cx.run_until_parked();
    let drawn = bar
        .read(|view| PathAdapter { view }.call("pathSuggestionRows", &[]))
        .unwrap()
        .stdout;
    let paths = bar.read(|view| {
        view.jump
            .entries
            .iter()
            .map(|row| row.text.clone())
            .collect()
    });
    (drawn.trim_end().to_owned(), paths)
}

#[gpui::test]
fn the_shipped_ranker_draws_the_boards_seven_rows_for_o(cx: &mut TestAppContext) {
    let (drawn, paths) = suggest(cx, Box::new(Jump::default()), "o");
    let want = [
        "~/Documents",
        "~/Downloads",
        "~/Projects",
        "~/Documents/claude/omarchy",
        "~/Pictures/screenshots",
        "~/Documents/claude/flea",
        "~/Work/field",
    ];
    assert_eq!(paths, want);
    assert_eq!(
        drawn, "7",
        "pathSuggestionRows reads the rows the popup laid and painted"
    );
}

#[gpui::test]
fn the_seam_stand_in_draws_no_row_and_the_reader_says_so(cx: &mut TestAppContext) {
    let (drawn, paths) = suggest(cx, Box::new(Unranked), "o");
    assert!(paths.is_empty());
    assert_eq!(
        drawn, "0",
        "a bar that draws no suggestion reads 0, so a scene waiting on rows fails"
    );
}

#[gpui::test]
fn the_reader_falls_to_zero_when_the_popup_goes(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Jump::default()), inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(board()), window, cx));
    bar.cx.simulate_input("s");
    bar.cx.run_until_parked();
    let rows = |bar: &mut super::view::Bar<'_>| {
        bar.read(|view| PathAdapter { view }.call("pathSuggestionRows", &[]))
            .unwrap()
            .stdout
    };
    // Six board folders hold an s, and a 360 px window shows them all.
    assert_eq!(rows(&mut bar).trim_end(), "6");
    bar.cx.simulate_input("/");
    bar.cx.run_until_parked();
    assert_eq!(rows(&mut bar).trim_end(), "0", "a path line lists nothing");
    bar.cx.simulate_keystrokes("escape");
    bar.cx.run_until_parked();
    assert_eq!(rows(&mut bar).trim_end(), "0");
}

#[gpui::test]
fn the_cursor_reader_follows_the_highlight_and_reads_minus_one_with_no_popup(
    cx: &mut TestAppContext,
) {
    let mut bar = bar(cx, Box::new(Jump::default()), inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(board()), window, cx));
    bar.cx.simulate_input("o");
    bar.cx.run_until_parked();
    let cursor = |bar: &mut super::view::Bar<'_>| {
        bar.read(|view| PathAdapter { view }.call("pathSuggestionCursor", &[]))
            .unwrap()
            .stdout
    };
    assert_eq!(
        cursor(&mut bar).trim_end(),
        "0",
        "the first row stands highlighted"
    );
    bar.cx.simulate_keystrokes("down down");
    assert_eq!(
        cursor(&mut bar).trim_end(),
        "2",
        "each Down steps the highlight one row"
    );
    bar.cx.simulate_keystrokes("escape");
    bar.cx.run_until_parked();
    assert_eq!(
        cursor(&mut bar).trim_end(),
        "-1",
        "a bar with no popup reads -1"
    );
}

#[gpui::test]
fn the_hover_takes_the_cursor_only_on_a_second_distinct_move(cx: &mut TestAppContext) {
    // The FIX6 hover scenes (200,92) then (200,128): row 1 is y 73 to 110, row 2 y 110 to 147.
    let mut bar = bar(cx, Box::new(Jump::default()), inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(board()), window, cx));
    bar.cx.simulate_input("o");
    bar.cx.run_until_parked();
    let cursor = |bar: &mut super::view::Bar<'_>| {
        bar.read(|view| PathAdapter { view }.call("pathSuggestionCursor", &[]))
            .unwrap()
            .stdout
    };
    assert_eq!(cursor(&mut bar).trim_end(), "0");
    assert!(
        bar.cx.update(|window, cx| window.simulate_next_frame(cx)) > 0,
        "the rows settle before the pointer arrives"
    );
    bar.cx
        .simulate_mouse_move(point(px(200.), px(92.)), None, Modifiers::none());
    bar.cx.run_until_parked();
    assert_eq!(
        cursor(&mut bar).trim_end(),
        "0",
        "the entry sighting over row 1 takes nothing"
    );
    bar.cx
        .simulate_mouse_move(point(px(200.), px(92.)), None, Modifiers::none());
    bar.cx.run_until_parked();
    assert_eq!(
        cursor(&mut bar).trim_end(),
        "0",
        "the same point again takes nothing"
    );
    bar.cx
        .simulate_mouse_move(point(px(200.), px(128.)), None, Modifiers::none());
    bar.cx.run_until_parked();
    assert_eq!(
        cursor(&mut bar).trim_end(),
        "2",
        "a distinct point in row 2 takes the cursor"
    );
}

#[gpui::test]
fn the_pointer_reader_reports_the_last_seen_point_or_none(cx: &mut TestAppContext) {
    // The popup stands at (80, 30), so entry (200, 92) reads (120, 62); a dropped x origin reads (200, 62).
    let mut bar = bar_at(cx, Box::new(Jump::default()), inputs(HOME), 80.);
    let pointer = |bar: &mut super::view::Bar<'_>| {
        bar.read(|view| PathAdapter { view }.call("pathSuggestionPointer", &[]))
            .unwrap()
            .stdout
    };
    assert_eq!(
        pointer(&mut bar).trim_end(),
        "none",
        "no popup, no sighting"
    );
    bar.update(|view, window, cx| view.open(window, cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(board()), window, cx));
    bar.cx.simulate_input("o");
    bar.cx.run_until_parked();
    assert_eq!(
        pointer(&mut bar).trim_end(),
        "none",
        "the popup opens with no sighting"
    );
    assert!(bar.cx.update(|window, cx| window.simulate_next_frame(cx)) > 0);
    assert_eq!(
        pointer(&mut bar).trim_end(),
        "none",
        "a settled popup still saw no pointer"
    );
    bar.cx
        .simulate_mouse_move(point(px(200.), px(92.)), None, Modifiers::none());
    bar.cx.run_until_parked();
    assert_eq!(
        pointer(&mut bar).trim_end(),
        "120 62",
        "the entry point in popup coordinates"
    );
    bar.cx.simulate_keystrokes("escape");
    bar.cx.run_until_parked();
    assert_eq!(
        pointer(&mut bar).trim_end(),
        "none",
        "closing the popup drops the sighting"
    );
    bar.update(|view, window, cx| view.open(window, cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(board()), window, cx));
    bar.cx.simulate_input("o");
    bar.cx.run_until_parked();
    assert_eq!(
        pointer(&mut bar).trim_end(),
        "none",
        "reopening starts with no sighting"
    );
}
