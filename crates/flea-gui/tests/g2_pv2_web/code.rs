//! The code view (quicklook-code.qml, codelines.js, CodeRefusal.qml): one ask per screen and margin, each line shaped
//! once while in view, wrap keeping the top line, and every refusal class drawing its fixed sentence or frame.
use super::parts::{catalog, draw, mark, palette, typeface, with, Calls, METRICS};
use flea::ui_logic_doc_preview::code_view::{Answer, Ask, Reply, Runs, BYTE_GATE, LINE_CAP};
use flea_gui::g2::doc_preview::code::{shape, CodeHost, CodeView};
use gpui::{TestAppContext, WindowHandle};

struct Host(Calls<Ask>);

impl CodeHost for Host {
    fn ask_code(&self, _: &str, ask: &Ask) -> u64 {
        self.0.borrow_mut().push(ask.clone());
        self.0.borrow().len() as u64
    }
}

fn open(cx: &mut TestAppContext) -> (WindowHandle<CodeView>, Calls<Ask>) {
    let calls = Calls::default();
    let host = Host(calls.clone());
    let view = cx.add_window(|_, cx| {
        let mut view = CodeView::new("look", typeface(), palette(), METRICS, mark("rust"));
        view.set_host(Some(Box::new(host)), cx);
        view.set_catalog(catalog(), cx);
        view
    });
    draw(cx, view);
    (view, calls)
}

fn show(cx: &mut TestAppContext, view: WindowHandle<CodeView>, size: u64) {
    view.update(cx, |v, _, cx| {
        v.set_active(true, cx);
        v.set_subject("/tmp/main.rs", size, false, None, cx);
    })
    .unwrap();
    draw(cx, view);
}

fn line(n: usize) -> Runs {
    vec![(4, format!("fn line_{n}")), (0, "() {}".into())]
}

fn answer(
    cx: &mut TestAppContext,
    view: WindowHandle<CodeView>,
    token: u64,
    state: Answer,
    ask: &Ask,
    total: usize,
) {
    let lines = (ask.first..ask.first + ask.count)
        .filter(|n| *n <= total)
        .map(line)
        .collect();
    view.update(cx, |v, _, cx| {
        v.reply(
            Reply {
                token,
                state,
                first: ask.first,
                total,
                lines,
            },
            cx,
        )
    })
    .unwrap();
    draw(cx, view);
}

#[gpui::test]
fn pv2_code_first_screen_asks_once_and_shapes_only_the_view(cx: &mut TestAppContext) {
    let (view, calls) = open(cx);
    show(cx, view, 40_000);
    assert_eq!(calls.borrow().len(), 1, "the first screen is one ask");
    let ask = calls.borrow()[0].clone();
    let rows = with(cx, view, |v| v.view().rows);
    assert_eq!(ask.first, 1, "from the top");
    assert!(
        ask.count <= LINE_CAP && ask.count >= rows,
        "a screen and a margin, never the file: {} for {rows} rows",
        ask.count
    );
    answer(cx, view, 1, Answer::Styled, &ask, 3000);
    let (shaped, cached, first, held) = with(cx, view, |v| {
        (
            v.cache.shaped(),
            v.cache.len(),
            v.first_screen(),
            v.lines.held(),
        )
    });
    assert!(first, "the first screen is whole");
    assert!(
        shaped as usize <= rows + 1,
        "only the lines in view are shaped: {shaped} for {rows} rows"
    );
    assert!(
        cached <= rows + 1 && held <= ask.count,
        "and held: {cached} shaped, {held} lines"
    );
    assert!(with(cx, view, |v| v.painted.get()) > 0, "the view painted");
    draw(cx, view);
    draw(cx, view);
    assert_eq!(
        with(cx, view, |v| v.cache.shaped()),
        shaped,
        "an idle redraw reshapes nothing"
    );
    view.update(cx, |v, _, cx| v.scroll_rows(3, cx)).unwrap();
    draw(cx, view);
    view.update(cx, |v, _, cx| v.scroll_rows(-3, cx)).unwrap();
    draw(cx, view);
    assert_eq!(
        with(cx, view, |v| v.cache.shaped()),
        shaped + 3,
        "three rows down and back shape only the three new lines"
    );
    let label = with(cx, view, |v| v.count_label(&catalog()));
    assert_eq!(
        label.as_deref(),
        Some("3,000 lines"),
        "the bar's count is the file's whole total"
    );
}

#[gpui::test]
fn pv2_code_scroll_asks_the_next_window_and_drops_stale_tokens(cx: &mut TestAppContext) {
    let (view, calls) = open(cx);
    show(cx, view, 400_000);
    let ask = calls.borrow()[0].clone();
    answer(cx, view, 1, Answer::Styled, &ask, 20_000);
    view.update(cx, |v, _, cx| v.scroll_rows(1_000, cx))
        .unwrap();
    draw(cx, view);
    let asks = calls.borrow().len();
    assert_eq!(asks, 2, "a far jump asks once for its window");
    let next = calls.borrow()[1].clone();
    let top = with(cx, view, |v| v.top);
    assert!(
        next.first <= top + 1 && next.first + next.count > top + 1,
        "the window covers line {}",
        top + 1
    );
    answer(cx, view, 1, Answer::Styled, &ask, 20_000);
    assert!(
        !with(cx, view, |v| v.first_screen()),
        "the old token's lines never fill the new window"
    );
    answer(cx, view, 2, Answer::Styled, &next, 20_000);
    assert!(with(cx, view, |v| v.first_screen()), "the new token's do");
    assert_eq!(calls.borrow().len(), 2, "and nothing more is asked at rest");
}

#[gpui::test]
fn pv2_code_wrap_keeps_the_top_line_and_drops_the_continuation_numbers(cx: &mut TestAppContext) {
    let (view, calls) = open(cx);
    show(cx, view, 40_000);
    let ask = calls.borrow()[0].clone();
    view.update(cx, |v, _, cx| {
        let long: Vec<Runs> = (ask.first..ask.first + ask.count)
            .map(|n| vec![(0, format!("{n} {}", "word ".repeat(80)))])
            .collect();
        v.reply(
            Reply {
                token: 1,
                state: Answer::Plain,
                first: ask.first,
                total: 200,
                lines: long,
            },
            cx,
        );
        v.scroll_rows(30, cx);
    })
    .unwrap();
    draw(cx, view);
    let top = with(cx, view, |v| v.top);
    assert_eq!(top, 30, "thirty rows down is line 31 at the top");
    view.update(cx, |v, _, cx| v.set_wrap(true, cx)).unwrap();
    draw(cx, view);
    let (wrapped_top, rows) = with(cx, view, |v| (v.top, v.rows_of(v.top)));
    assert_eq!(wrapped_top, top, "wrapping keeps line 31 at the top");
    assert!(rows > 1, "a long line wraps onto continuation rows: {rows}");
    view.update(cx, |v, _, cx| v.set_wrap(false, cx)).unwrap();
    draw(cx, view);
    assert_eq!(
        with(cx, view, |v| v.top),
        top,
        "and unwrapping keeps it too"
    );
}

#[gpui::test]
fn pv2_code_refusals_draw_their_fixed_sentences(cx: &mut TestAppContext) {
    let i18n = catalog();
    let (view, calls) = open(cx);
    show(cx, view, BYTE_GATE + 1);
    assert!(calls.borrow().is_empty(), "past the gate nothing is asked");
    let (frame, status, sentence) = with(cx, view, |v| {
        (v.frame_text(&i18n), v.status(&i18n), v.fail_sentence(&i18n))
    });
    assert_eq!(
        frame.as_deref(),
        Some("too large to preview"),
        "the too-large frame"
    );
    assert_eq!(status, "This file is too large to preview.");
    assert_eq!(sentence, None, "a frame draws instead of the sentence");
    for (state, frame, status, sentence) in [
        (Answer::Binary, Some("binary file"), "ready", None),
        (
            Answer::Large,
            Some("too large to preview"),
            "This file is too large to preview.",
            None,
        ),
        (
            Answer::Failed,
            None,
            "This file could not be read.",
            Some("This file could not be read."),
        ),
    ] {
        let (view, calls) = open(cx);
        show(cx, view, 2_000);
        let ask = calls.borrow()[0].clone();
        view.update(cx, |v, _, cx| {
            v.reply(
                Reply {
                    token: 1,
                    state,
                    first: ask.first,
                    total: 0,
                    lines: Vec::new(),
                },
                cx,
            )
        })
        .unwrap();
        draw(cx, view);
        let got = with(cx, view, |v| {
            (v.frame_text(&i18n), v.status(&i18n), v.fail_sentence(&i18n))
        });
        assert_eq!(
            got,
            (
                frame.map(String::from),
                status.to_owned(),
                sentence.map(String::from)
            ),
            "{state:?}"
        );
        assert_eq!(
            calls.borrow().len(),
            1,
            "{state:?} ends the file: nothing more is asked"
        );
    }
    let (view, _) = open(cx);
    show(cx, view, 2_000);
    view.update(cx, |v, _, cx| v.set_read_failed(cx)).unwrap();
    assert_eq!(
        with(cx, view, |v| v.fail_sentence(&i18n)).as_deref(),
        Some("This file could not be read."),
        "a failed remote read"
    );
}

#[gpui::test]
fn pv2_code_plain_text_draws_no_gutter(cx: &mut TestAppContext) {
    let (view, calls) = open(cx);
    view.update(cx, |v, _, _| v.numbered = false).unwrap();
    show(cx, view, 4_000);
    let ask = calls.borrow()[0].clone();
    answer(cx, view, 1, Answer::Plain, &ask, 50);
    let (numbers, left, inset) = with(cx, view, |v| {
        let advance = gpui::px(8.);
        assert_eq!(v.gutter_right(advance), v.metrics.inset);
        (v.cache.shaped(), v.text_left(advance), v.metrics.inset)
    });
    assert!(numbers > 0, "the lines were shaped");
    assert_eq!(left, inset, "plain text starts at the frame's inset");
    let shaped_numbers = with(cx, view, |v| {
        (0..v.lines.shown())
            .filter(|line| v.has_number(*line))
            .count()
    });
    assert_eq!(shaped_numbers, 0, "and no line carries a number");
}

#[gpui::test]
fn pv2_code_wrapped_long_line_shapes_and_paints_only_the_rows_in_view(cx: &mut TestAppContext) {
    let (view, calls) = open(cx);
    show(cx, view, 1_000_000);
    let ask = calls.borrow()[0].clone();
    let text = "word ".repeat(200_000);
    view.update(cx, |v, _, cx| {
        let lines = vec![vec![(0, text)]];
        v.reply(
            Reply {
                token: 1,
                state: Answer::Plain,
                first: ask.first,
                total: 1,
                lines,
            },
            cx,
        );
        v.set_wrap(true, cx);
    })
    .unwrap();
    draw(cx, view);
    let rows = with(cx, view, |v| v.view().rows);
    let (shaped, painted, frames, wraps) = with(cx, view, |v| {
        (
            v.cache.shaped() as usize,
            v.painted_rows.get(),
            v.painted.get(),
            v.wraps.get(),
        )
    });
    assert!(
        shaped <= rows + 1,
        "one minified line wrapped shapes only the rows in view: {shaped} for {rows}"
    );
    assert!(
        painted <= (rows + 1) * frames,
        "and paints only them: {painted} rows over {frames} frames for {rows}"
    );
    draw(cx, view);
    let (again, painted_again, frames_again, wraps_again) = with(cx, view, |v| {
        (
            v.cache.shaped() as usize,
            v.painted_rows.get(),
            v.painted.get(),
            v.wraps.get(),
        )
    });
    assert_eq!(again, shaped, "an idle redraw shapes nothing");
    assert!(
        painted_again - painted <= (rows + 1) * (frames_again - frames),
        "and paints only the rows in view again"
    );
    assert_eq!(wraps_again, wraps, "and never wraps the line again");
    view.update(cx, |v, _, cx| v.scroll_rows(2_000, cx))
        .unwrap();
    draw(cx, view);
    let (deep, top, offset) = with(cx, view, |v| (v.cache.shaped() as usize, v.top, v.offset));
    // Quick Look measures its row pitch from the body font (EY-170), so the step is the view's own row.
    assert_eq!(
        (top, offset),
        (0, 2_000. * with(cx, view, |v| v.metrics.row)),
        "two thousand rows into the one line"
    );
    assert!(
        deep > again && deep - again <= rows + 1,
        "rows far into the line are shaped on demand, only those in view: {} for {rows}",
        deep - again
    );
}

#[gpui::test]
fn pv2_code_unchanged_frame_builds_and_shapes_nothing(cx: &mut TestAppContext) {
    let (view, calls) = open(cx);
    show(cx, view, 40_000);
    let ask = calls.borrow()[0].clone();
    answer(cx, view, 1, Answer::Styled, &ask, 3000);
    let counts = |cx: &mut TestAppContext| {
        with(cx, view, |v| {
            (v.built.get(), v.cache.shaped(), shape::shaped_bytes())
        })
    };
    let before = counts(cx);
    for _ in 0..3 {
        draw(cx, view);
    }
    let after = counts(cx);
    assert_eq!(
        after, before,
        "three idle redraws of the lines build no frame and shape nothing (built, rows, bytes)"
    );
    view.update(cx, |v, _, cx| v.scroll_rows(1, cx)).unwrap();
    draw(cx, view);
    let moved = counts(cx);
    assert!(
        moved.0 > after.0 && moved.1 > after.1,
        "a scroll builds its frame and shapes the row entering: {moved:?} after {after:?}"
    );
    show(cx, view, BYTE_GATE + 1);
    let before = counts(cx);
    for _ in 0..3 {
        draw(cx, view);
    }
    let after = counts(cx);
    assert_eq!(
        after, before,
        "three idle redraws of the refusal frame build nothing and shape no caption"
    );
}
