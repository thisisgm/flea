//! Source and fences: lines asked a screen at a time from the code engine or drawn plain, CRLF ends dropped, and fences that hold the first screen until coloured.
use super::harness::{draw, first_screen, open, open_with, show, until, with, Helper, TOKENS};
use super::{file, root};
use flea_gui::g2::pv2::markdown::{CodeAnswer, CodeHost, CodeSource, CodeState, Tokens, View};
use gpui::TestAppContext;
use std::{cell::RefCell, rc::Rc};

type Asks = Rc<RefCell<Vec<(u64, String, CodeSource, usize, usize)>>>;

// Records every ask and hands out tokens; the test answers them through `code_answered`.
struct Engine(Asks);

impl CodeHost for Engine {
    fn ask_code(&self, slot: &str, source: CodeSource, first: usize, count: usize) -> u64 {
        let mut asks = self.0.borrow_mut();
        let token = asks.len() as u64 + 1;
        asks.push((token, slot.to_string(), source, first, count));
        token
    }
}

#[gpui::test]
fn md_source_numbers_plain_lines_and_lays_out_only_the_screen(cx: &mut TestAppContext) {
    let root = root();
    let mut text = String::from("# Title\r\n");
    for i in 0..3000 {
        text.push_str(&format!("line {i} of the source\r\n"));
    }
    let path = file(&root, "long.md", text.as_bytes());
    let view = open(cx, &root, Helper::Draws);
    draw(cx, view);
    first_screen(cx, view, &path);
    assert_eq!(
        with(cx, view, |v| v.source.laid_chars),
        0,
        "Rendered lays out no Source line"
    );
    view.update(cx, |v, _, cx| v.set_view(View::Source, cx))
        .unwrap();
    until(
        cx,
        view,
        "the Source lines",
        |_| {},
        |v| v.source.laid_chars > 0,
    );
    let (total, laid, first, height) = with(cx, view, |v| {
        (
            v.source.total,
            v.source.laid_chars,
            v.source.row(1),
            v.frame.1,
        )
    });
    assert_eq!(total, 3001, "every line is numbered");
    assert_eq!(
        first.unwrap(),
        vec![(0, "# Title".to_string())],
        "a CRLF line drops its carriage return"
    );
    let rows = (height / 14.).ceil() as usize + 1;
    assert!(
        laid <= rows * 30,
        "only the screen's lines are laid out: {laid} characters"
    );
    view.update(cx, |v, _, cx| {
        for _ in 0..100 {
            v.action("cursorDown", cx);
        }
    })
    .unwrap();
    draw(cx, view);
    let top = with(cx, view, |v| v.source.top);
    assert!(top > 50, "j scrolls Source a line at a time: top row {top}");
    view.update(cx, |v, _, cx| v.action("toggleWrap", cx))
        .unwrap();
    draw(cx, view);
    assert_eq!(
        with(cx, view, |v| (v.wrap, v.source.top)),
        (true, top),
        "w wraps and keeps the top line"
    );
}

#[gpui::test]
fn md_source_asks_the_engine_a_screen_at_a_time_and_shows_its_refusal(cx: &mut TestAppContext) {
    let root = root();
    let path = file(&root, "doc.md", b"# One\n\nTwo\n");
    let asks: Asks = Rc::default();
    let view = open(cx, &root, Helper::Draws);
    view.update(cx, |v, _, cx| {
        v.set_view(View::Source, cx);
        v.set_code_host(Box::new(Engine(asks.clone())), cx);
    })
    .unwrap();
    draw(cx, view);
    show(cx, view, &path);
    until(
        cx,
        view,
        "the first ask",
        |_| {},
        |_| !asks.borrow().is_empty(),
    );
    let (token, slot, source, from, count) = asks.borrow()[0].clone();
    assert_eq!(slot, "look");
    assert_eq!(
        source,
        CodeSource::Path(path.to_string_lossy().into_owned()),
        "the engine reads the file itself"
    );
    assert_eq!(from, 1);
    assert!(
        (1..=400).contains(&count),
        "one ask names at most 400 lines: {count}"
    );
    assert!(
        with(cx, view, |v| v.counts.first.is_none()),
        "Source waits for its lines"
    );
    let answer = CodeAnswer {
        token,
        state: CodeState::Styled,
        total: 3,
        first: 1,
        lines: vec![vec![(4, "# One".into())], vec![], vec![(0, "Two".into())]],
    };
    view.update(cx, |v, _, cx| v.code_answered(answer, cx))
        .unwrap();
    until(
        cx,
        view,
        "the coloured lines",
        |_| {},
        |v| v.source.laid_chars == 8,
    );
    // A binary answer replaces the lines with the refusal's mark and caption.
    let path = file(&root, "blob.md", b"\0\0\0");
    show(cx, view, &path);
    until(
        cx,
        view,
        "the second ask",
        |_| {},
        |_| asks.borrow().len() >= 2,
    );
    let token = asks.borrow().last().unwrap().0;
    let refusal = CodeAnswer {
        token,
        state: CodeState::Binary,
        total: 0,
        first: 1,
        lines: Vec::new(),
    };
    view.update(cx, |v, _, cx| v.code_answered(refusal, cx))
        .unwrap();
    draw(cx, view);
    assert_eq!(
        with(cx, view, |v| v.source_refused()),
        Some(flea_gui::g2::pv2::markdown::Answer::Binary)
    );
    assert_eq!(
        with(cx, view, |v| v.status_sentence().map(str::to_string)),
        None,
        "a binary file has its caption, not a sentence"
    );
}

#[gpui::test]
fn md_a_named_fence_holds_the_first_screen_until_coloured(cx: &mut TestAppContext) {
    let root = root();
    let path = file(
        &root,
        "code.md",
        b"Before\n\n```rust\nfn main() {}\n```\n\n```\nplain\n```\n",
    );
    let asks: Asks = Rc::default();
    let view = open(cx, &root, Helper::Draws);
    view.update(cx, |v, _, cx| {
        v.set_code_host(Box::new(Engine(asks.clone())), cx)
    })
    .unwrap();
    draw(cx, view);
    show(cx, view, &path);
    until(
        cx,
        view,
        "the fence's ask",
        |_| {},
        |_| !asks.borrow().is_empty(),
    );
    draw(cx, view);
    assert!(
        with(cx, view, |v| v.counts.first.is_none()),
        "the first screen waits for the fence's colours"
    );
    let (token, slot, source, from, count) = asks.borrow()[0].clone();
    assert_eq!(slot, "look:fence:0");
    assert_eq!(
        source,
        CodeSource::Text {
            text: "fn main() {}".into(),
            name: String::new(),
            lang: "rust".into()
        }
    );
    assert_eq!((from, count), (1, 1));
    assert_eq!(
        asks.borrow().len(),
        1,
        "an unnamed fence asks for no colours"
    );
    let answer = CodeAnswer {
        token,
        state: CodeState::Styled,
        total: 1,
        first: 1,
        lines: vec![vec![(5, "fn".into()), (0, " main() {}".into())]],
    };
    view.update(cx, |v, _, cx| v.code_answered(answer, cx))
        .unwrap();
    until(
        cx,
        view,
        "the first screen",
        |_| {},
        |v| v.counts.first.is_some(),
    );
    let first = with(cx, view, |v| v.counts.first.clone().unwrap());
    assert_eq!(
        first.fences, 2,
        "both fences are drawn on their ground: {first:?}"
    );
}

#[gpui::test]
fn md_a_document_nested_past_the_limit_shows_its_source_behind_a_notice(cx: &mut TestAppContext) {
    let root = root();
    let deep = format!("{}deep\n", "> ".repeat(40));
    let path = file(&root, "deep.md", deep.as_bytes());
    let view = open(cx, &root, Helper::Draws);
    draw(cx, view);
    first_screen(cx, view, &path);
    let (shown, notice, total) = with(cx, view, |v| {
        (
            v.shown_view(),
            v.source.notice().map(str::to_string),
            v.source.total,
        )
    });
    assert_eq!(shown, View::Source, "a too-deep document shows its source");
    assert_eq!(
        notice.as_deref(),
        Some("This file nests deeper than 32 levels, so Flea shows its source.")
    );
    assert_eq!(total, 1);
}

#[gpui::test]
fn md_source_board_row_never_draws_under_a_larger_face(cx: &mut TestAppContext) {
    let root = root();
    let path = file(&root, "large.md", b"# Title\n\nA line.\n");
    let large = Tokens {
        body: 20.,
        theme_body: 20.,
        ..TOKENS
    };
    let line = |source_line, cx: &mut TestAppContext| {
        let view = open_with(
            cx,
            &root,
            Helper::Draws,
            Tokens {
                source_line,
                ..large
            },
        );
        draw(cx, view);
        first_screen(cx, view, &path);
        view.update(cx, |v, _, cx| v.set_view(View::Source, cx))
            .unwrap();
        until(
            cx,
            view,
            "the Source lines",
            |_| {},
            |v| v.source.laid_chars > 0,
        );
        with(cx, view, |v| v.source.frame.line)
    };
    // A zero row reads the face's extent through the board path; None takes the column's body line box.
    let face = line(Some(0.), cx);
    assert!(
        face > 20.,
        "a 20 px face is taller than the board's 20 px row: {face}"
    );
    assert_eq!(
        line(Some(20.), cx),
        face,
        "the board's row gives way to the face's extent"
    );
    let column = line(None, cx);
    assert_eq!(
        column, 28.,
        "the column takes the body's 1.4 line box, not the face's {face}"
    );
    assert_ne!(
        face, column,
        "the board path reads the face, not the column's line box"
    );
}
