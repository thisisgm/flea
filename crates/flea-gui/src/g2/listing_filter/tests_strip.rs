//! The strip's geometry against boards.py filter_strip, and its shaping cost, from a real GPUI frame.
use super::present::{self, StripText};
use super::strip::{Laid, StripState, HEIGHT};
use super::StripView;
use crate::list::Typeface;
use flea::ui_logic_listing_filter::{Counts, Scope};
use gpui::{prelude::*, px, Pixels, TestAppContext, WindowHandle};
use std::{cell::RefCell, rc::Rc};

fn open(
    cx: &mut TestAppContext,
    text: StripText,
) -> (Rc<RefCell<StripState>>, WindowHandle<StripView>) {
    let state = Rc::new(RefCell::new(StripState::new(Typeface::omarchy_default())));
    state.borrow_mut().text = text;
    let view = StripView {
        state: Rc::clone(&state),
    };
    (state, cx.add_window(|_, _| view))
}

fn frame(cx: &mut TestAppContext, handle: WindowHandle<StripView>) {
    cx.update_window(handle.into(), |_, window, _| window.refresh())
        .unwrap();
    cx.run_until_parked();
}

fn typing(query: &str, caret: bool) -> StripText {
    StripText {
        query: query.to_owned().into(),
        caret,
        summary: "0 of 9 shown · 1 close match".into(),
        way_out: "esc clears".into(),
    }
}

fn right(bounds: gpui::Bounds<Pixels>) -> Pixels {
    bounds.origin.x + bounds.size.width
}

#[gpui::test]
fn the_strip_lays_out_as_the_filter_board_draws_it(cx: &mut TestAppContext) {
    let (state, handle) = open(cx, typing("ntoes", true));
    frame(cx, handle);
    let laid: Laid = state.borrow().laid.expect("a frame lays the strip out");
    let width = laid.rule.size.width;
    assert_eq!(
        (laid.mark.origin.x, laid.mark.size.width),
        (px(14.), px(16.)),
        "the funnel sits 14 in and is 16 square"
    );
    assert_eq!(
        laid.query.origin.x,
        px(39.),
        "the query starts a 9 gap after the funnel"
    );
    let caret = laid.caret.expect("the caret draws while typing");
    assert_eq!(
        caret.origin.x,
        right(laid.query) + px(1.),
        "the caret sits 1 after the query"
    );
    assert_eq!(
        (caret.size.width, caret.size.height),
        (px(2.), px(13.)),
        "and is 2 by 13"
    );
    let summary = laid.summary.expect("the summary draws once counts stand");
    assert_eq!(
        summary.origin.x,
        right(caret) + px(9.),
        "the summary follows the caret by the 9 gap"
    );
    assert_eq!(
        right(laid.way_out),
        width - px(14.),
        "esc clears ends 14 from the right edge"
    );
    assert!(
        right(summary) <= laid.way_out.origin.x - px(9.),
        "the summary never runs under esc clears"
    );
    assert_eq!(
        (laid.rule.origin.y, laid.rule.size.height),
        (px(HEIGHT - 1.), px(1.)),
        "a 1 rule closes the 27 strip"
    );
    let mid = |bounds: gpui::Bounds<Pixels>| bounds.origin.y + bounds.size.height / 2.;
    assert_eq!(
        mid(laid.mark),
        mid(caret),
        "the funnel and the caret share one centre line"
    );
}

#[gpui::test]
fn the_caret_draws_only_while_typing_and_the_summary_then_follows_the_query(
    cx: &mut TestAppContext,
) {
    let (state, handle) = open(cx, typing("item-7?.txt", false));
    frame(cx, handle);
    let laid = state.borrow().laid.unwrap();
    assert_eq!(laid.caret, None, "a committed filter draws no caret");
    assert_eq!(
        laid.summary.unwrap().origin.x,
        right(laid.query) + px(9.),
        "and the summary follows the query itself"
    );
    state.borrow_mut().text.summary = "".into();
    frame(cx, handle);
    assert_eq!(
        state.borrow().laid.unwrap().summary,
        None,
        "a query still waiting on its answer draws no summary"
    );
}

// cols-filter-rest and grid-filter-rest: "/" with nothing typed draws the funnel, the caret and esc clears, and no summary.
#[gpui::test]
fn the_open_empty_line_draws_its_caret_where_the_query_will_start(cx: &mut TestAppContext) {
    let rest = StripText {
        query: "".into(),
        caret: true,
        summary: "".into(),
        way_out: "esc clears".into(),
    };
    let (state, handle) = open(cx, rest);
    frame(cx, handle);
    let laid = state.borrow().laid.unwrap();
    assert_eq!(
        laid.caret.map(|caret| caret.origin.x),
        Some(px(40.)),
        "the caret stands 1 after where the query starts"
    );
    assert_eq!(laid.summary, None, "nothing typed counts nothing");
    assert_eq!(
        right(laid.way_out),
        laid.rule.size.width - px(14.),
        "esc clears still sits at the right"
    );
}

#[gpui::test]
fn a_frame_at_rest_shapes_nothing_and_a_keystroke_shapes_only_what_changed(
    cx: &mut TestAppContext,
) {
    let (state, handle) = open(cx, typing("ntoe", true));
    frame(cx, handle);
    let first = state.borrow().shaped;
    assert_eq!(
        first, 3,
        "the first frame shapes the query, the summary and esc clears"
    );
    frame(cx, handle);
    assert_eq!(
        state.borrow().shaped,
        first,
        "a frame with the same words shapes nothing"
    );
    state.borrow_mut().text.query = "ntoes".into();
    frame(cx, handle);
    assert_eq!(
        state.borrow().shaped,
        first + 1,
        "a keystroke reshapes the query alone"
    );
}

#[test]
fn the_strip_words_come_from_the_filter_catalog() {
    let i18n = super::tests_flow::english();
    let counts = |shown, tested, close, scope| Counts {
        shown,
        tested,
        close,
        scope,
    };
    assert_eq!(
        present::summary(&counts(10, 80, 0, Scope::Folder), "work", &i18n),
        "10 of 80 shown"
    );
    assert_eq!(
        present::summary(&counts(0, 9, 1, Scope::Folder), "work", &i18n),
        "0 of 9 shown · 1 close match"
    );
    assert_eq!(
        present::summary(&counts(0, 9, 2, Scope::Folder), "work", &i18n),
        "0 of 9 shown · 2 close matches"
    );
    let deep = counts(2, 119, 0, Scope::Subfolders);
    assert_eq!(
        present::summary(&deep, "work", &i18n),
        "2 of 119 shown in work and subfolders"
    );
    assert_eq!(
        present::status(&deep, &i18n),
        "2 of 119 shown",
        "the status bar leaves the scope to the strip"
    );
    assert_eq!(
        present::summary(&counts(1200, 1300, 0, Scope::Folder), "work", &i18n),
        "1,200 of 1,300 shown"
    );
    assert_eq!(i18n.tr("filter.esc_clears"), "esc clears");
    assert_eq!(
        i18n.tr_args("filter.nothing_matches", &["zzz"]),
        "NOTHING MATCHES zzz"
    );
}

#[test]
fn a_typed_run_washes_at_rest_bolds_on_the_cursor_and_leaves_a_mark_alone() {
    use present::{run_style, RunStyle};
    assert_eq!(run_style(false, false), RunStyle::Wash);
    assert_eq!(run_style(true, false), RunStyle::Bold);
    assert_eq!(run_style(false, true), RunStyle::Plain);
    assert_eq!(run_style(true, true), RunStyle::Plain);
    let runs = [flea::ui_logic_listing_filter::Run { start: 1, len: 2 }];
    assert_eq!(
        present::byte_ranges("\u{e9}t\u{e9}x", &runs),
        vec![2..5],
        "char runs cut the name on its byte boundaries"
    );
    assert!(
        present::byte_ranges(
            "ab",
            &[flea::ui_logic_listing_filter::Run { start: 1, len: 5 }]
        )
        .is_empty(),
        "a run past the name is dropped"
    );
}

#[test]
fn the_run_cache_holds_the_window_on_screen_and_drops_whole_on_a_new_query() {
    let mut cache = present::RunCache::new(2);
    assert_eq!(
        &*cache.get("no", 1, 0, "notes.txt", false),
        &[flea::ui_logic_listing_filter::Run { start: 0, len: 2 }]
    );
    let again = cache.get("no", 1, 0, "notes.txt", false);
    assert_eq!(cache.len(), 1, "a row asked twice is matched once");
    let _ = cache.get("no", 1, 1, "photo.png", false);
    let _ = cache.get("no", 1, 2, "nope", false);
    assert_eq!(cache.len(), 2, "the cache never outgrows its window");
    assert_eq!(again.len(), 1);
    let close = cache.get("ntoes", 1, 6, "notes.txt", true);
    assert_eq!(cache.len(), 1, "a new query drops every old run");
    assert!(!close.is_empty(), "a close row washes its aligned letters");
    let _ = cache.get("ntoes", 2, 6, "notes.txt", true);
    assert_eq!(cache.len(), 1, "and so does a new numbering");
}

// FilterStrip.qml's summary elides right, so a long sentence ends in an ellipsis before esc clears and is never cut mid-glyph.
#[gpui::test]
fn a_summary_too_long_for_its_room_elides_right(cx: &mut TestAppContext) {
    let long = format!("0 of 9 shown \u{b7} {}", "close ".repeat(120));
    let (state, handle) = open(
        cx,
        StripText {
            summary: long.clone().into(),
            ..typing("ntoes", true)
        },
    );
    frame(cx, handle);
    let laid = state.borrow().laid.unwrap();
    let drawn = state
        .borrow()
        .summary_drawn()
        .cloned()
        .expect("the summary draws");
    assert!(
        drawn.ends_with('\u{2026}'),
        "the sentence ends in an ellipsis: {drawn}"
    );
    assert!(
        long.starts_with(drawn.trim_end_matches('\u{2026}')),
        "after the words that fit"
    );
    assert!(
        right(laid.summary.unwrap()) <= laid.way_out.origin.x - px(9.),
        "and stops the 9 gap before esc clears"
    );
    let shaped = state.borrow().shaped;
    frame(cx, handle);
    assert_eq!(
        state.borrow().shaped,
        shaped,
        "a frame at rest shapes the elided sentence no more"
    );
    state.borrow_mut().text.summary = "0 of 9 shown".into();
    frame(cx, handle);
    assert_eq!(
        state.borrow().summary_drawn().map(|text| text.as_ref()),
        Some("0 of 9 shown"),
        "a sentence that fits draws whole"
    );
}

// A system font change reshapes every line and recuts the elided summary, so no width measured in the old face survives.
#[gpui::test]
fn a_font_change_reshapes_every_line_and_recuts_the_summary(cx: &mut TestAppContext) {
    let long = format!("0 of 9 shown \u{b7} {}", "close ".repeat(120));
    let (state, handle) = open(
        cx,
        StripText {
            summary: long.into(),
            ..typing("ntoes", true)
        },
    );
    frame(cx, handle);
    let shaped = state.borrow().shaped;
    state.borrow_mut().typeface.font.family = "DejaVu Serif".into();
    frame(cx, handle);
    // The ellipsis is shaped only by a recut, so its reshape is what proves the summary was cut again in the new face.
    assert_eq!(
        state.borrow().shaped,
        shaped + 5,
        "the query, summary, way out, ellipsis and elided summary reshape in the new face"
    );
    frame(cx, handle);
    assert_eq!(
        state.borrow().shaped,
        shaped + 5,
        "and a frame at rest after it shapes nothing"
    );
}
