use super::*;
use crate::ui_logic_doc_preview::refusal::Refusal;

fn run(text: &str) -> Runs {
    vec![(0, text.to_string())]
}

fn engine(path: &str) -> Lines {
    let mut lines = Lines::new(1);
    lines.engine = true;
    lines.active = true;
    lines.set_subject(path, false, None);
    lines
}

fn reply(token: u64, state: Answer, first: usize, total: usize, count: usize) -> Reply {
    Reply {
        token,
        state,
        first,
        total,
        lines: (first..first + count)
            .map(|n| run(&format!("fn f{n}()")))
            .collect(),
    }
}

// tests/quicklook-code.qml: the first screen asked for one range, not the file.
#[test]
fn code_first_screen_asks_one_range() {
    let mut lines = engine("/w/main.rs");
    assert!(lines.due, "a new subject asks at the end of the turn");
    let view = Window { top: 1, rows: 30 };
    let ask = lines.ask(view).expect("the view asks");
    assert_eq!(
        (ask.first, ask.count),
        (1, 60),
        "the lines in view plus one screen of margin"
    );
    assert_eq!(
        ask.source,
        Source::Path("/w/main.rs".into()),
        "a local file is asked by path"
    );
    lines.sent(7);
    assert_eq!(
        lines.ask(view),
        None,
        "the same window is not asked twice while it is in flight"
    );
    assert!(lines.answered(reply(7, Answer::Styled, 1, 5000, 60)));
    assert!(lines.first_screen(view), "the lines in view are held");
    assert_eq!(lines.asks, 1, "one ask for the first screen");
    assert_eq!(
        lines.lines_taken, 60,
        "only the asked lines were taken, never the file"
    );
}

// Scrolling to the end asks for the lines in view and one screen above, and ends at the file's last line.
#[test]
fn code_scrolling_to_the_end_asks_the_view_and_one_screen_above() {
    let mut lines = engine("/w/icons.rs");
    lines.ask(Window { top: 1, rows: 30 }).unwrap();
    lines.sent(1);
    lines.answered(reply(1, Answer::Styled, 1, 1000, 60));
    let end = Window { top: 971, rows: 30 };
    let ask = lines.ask(end).expect("the end asks");
    assert_eq!(ask.first, 941, "one screen above the view");
    assert_eq!(
        ask.first + ask.count - 1,
        1000,
        "and never past the file's last line"
    );
    lines.sent(2);
    lines.answered(reply(2, Answer::Styled, 941, 1000, 60));
    assert!(lines.row(1000).is_some(), "the last line is held");
    assert!(
        lines.first_screen(end),
        "and the end's first screen is whole"
    );
}

#[test]
fn code_an_answer_for_another_token_is_dropped() {
    let mut lines = engine("/w/a.rs");
    lines.ask(Window { top: 1, rows: 10 }).unwrap();
    lines.sent(3);
    assert!(
        !lines.answered(reply(2, Answer::Styled, 1, 10, 10)),
        "a stale token is dropped"
    );
    assert_eq!(lines.answer, Answer::Pending, "and changes nothing");
    lines.set_subject("/w/b.rs", false, None);
    assert!(
        !lines.answered(reply(3, Answer::Styled, 1, 10, 10)),
        "a new subject drops the old ask's answer"
    );
    assert_eq!(lines.held(), 0, "nothing of the old file is held");
}

// tests/html-source-refusals.qml and quicklook-code.qml: binary, large and failed end the file and release the first screen.
#[test]
fn code_terminal_answers_end_the_file() {
    for (state, refusal) in [
        (Answer::Binary, Refusal::Binary),
        (Answer::Large, Refusal::TooLarge),
        (Answer::Failed, Refusal::Unreadable),
    ] {
        let mut lines = engine("/w/x.html");
        let view = Window { top: 1, rows: 20 };
        lines.ask(view).unwrap();
        lines.sent(9);
        assert!(lines.answered(Reply {
            token: 9,
            state,
            first: 1,
            total: 0,
            lines: vec![]
        }));
        assert!(!lines.due, "{state:?} asks nothing more");
        assert_eq!(
            lines.ask(view),
            None,
            "{state:?} is the file's whole answer"
        );
        assert!(
            lines.active,
            "{state:?} keeps the reader active to retain its terminal reply"
        );
        assert_eq!(
            super::refusal(1, BYTE_GATE, lines.answer, false),
            Some(refusal),
            "{state:?} uses the existing refusal state"
        );
    }
}

#[test]
fn code_refusal_order_and_gates() {
    assert_eq!(
        super::refusal(BYTE_GATE + 1, BYTE_GATE, Answer::Pending, false),
        Some(Refusal::TooLarge),
        "past 1 MiB by the listing's size"
    );
    assert_eq!(
        super::refusal(10, BYTE_GATE, Answer::Large, false),
        Some(Refusal::TooLarge),
        "or by the backend's own count"
    );
    assert_eq!(
        super::refusal(10, REMOTE_GATE, Answer::Pending, true),
        Some(Refusal::Unreadable),
        "a remote read that failed"
    );
    assert_eq!(
        super::refusal(10, BYTE_GATE, Answer::Styled, false),
        None,
        "a coloured answer draws its lines"
    );
    assert_eq!(
        super::refusal(10, BYTE_GATE, Answer::Plain, false),
        None,
        "a line over 5,000 bytes answers plain and still draws"
    );
}

// The remote route: the caller's bytes, drawn plain when no engine colours them, and the backend never given the path.
#[test]
fn code_remote_route_draws_the_files_own_lines() {
    let mut lines = Lines::new(1);
    lines.active = true;
    lines.set_subject("/nas/notes.txt", true, None);
    assert_eq!(
        lines.answer,
        Answer::Pending,
        "nothing is drawn until the caller has read the file"
    );
    lines.set_subject(
        "/nas/notes.txt",
        true,
        Some("one\r\n<img src=x>\na & b\n".into()),
    );
    assert_eq!(
        lines.answer,
        Answer::Plain,
        "a bare host leaves the caller's text drawn plain"
    );
    assert_eq!(
        lines.plain_lines(),
        vec!["one", "<img src=x>", "a & b"],
        "in order, the CR and the terminal newline dropped, markup as text"
    );
    let mut coloured = Lines::new(1);
    coloured.engine = true;
    coloured.active = true;
    coloured.set_subject("/nas/x.rs", true, Some("fn x() {}\n".into()));
    let ask = coloured.ask(Window { top: 1, rows: 5 }).unwrap();
    assert_eq!(
        ask.source,
        Source::Text {
            text: "fn x() {}\n".into(),
            name: "x.rs".into()
        },
        "the bytes go, never the path"
    );
    let mut empty = Lines::new(1);
    empty.engine = true;
    empty.active = true;
    empty.set_subject("/nas/empty.txt", true, Some(String::new()));
    assert_eq!(
        empty.ask(Window { top: 1, rows: 5 }),
        None,
        "an empty text asks nothing"
    );
    assert_eq!(
        (empty.answer, empty.total),
        (Answer::Plain, 0),
        "and draws no line"
    );
}

#[test]
fn code_inactive_and_retiring_views_ask_nothing() {
    let mut lines = engine("/w/a.rs");
    lines.set_active(false);
    assert!(!lines.due, "an inactive view cancels its due ask");
    assert_eq!(
        lines.ask(Window { top: 1, rows: 10 }),
        None,
        "and asks nothing"
    );
    let mut retiring = engine("/w/a.rs");
    retiring.retiring = true;
    assert_eq!(
        retiring.ask(Window { top: 1, rows: 10 }),
        None,
        "a view being destroyed never asks"
    );
    let mut zero = engine("/w/a.rs");
    assert_eq!(
        zero.ask(Window { top: 1, rows: 0 }),
        None,
        "a view with no height asks nothing"
    );
}

#[test]
fn code_a_long_window_is_asked_in_parts() {
    let mut lines = Lines::new(0);
    lines.engine = true;
    lines.active = true;
    lines.set_subject("/w/big.rs", false, None);
    let ask = lines.ask(Window { top: 1, rows: 900 }).unwrap();
    assert_eq!(ask.count, LINE_CAP, "one ask names at most the line cap");
}

// Performance gate: the lines held stay bounded however far the view scrolls.
#[test]
fn code_held_lines_stay_bounded() {
    let mut lines = engine("/w/long.rs");
    let total = 20_000;
    let mut token = 0;
    for top in (1..total).step_by(LINE_CAP) {
        let view = Window {
            top,
            rows: LINE_CAP / 2,
        };
        if let Some(ask) = lines.ask(view) {
            token += 1;
            lines.sent(token);
            lines.answered(reply(token, Answer::Styled, ask.first, total, ask.count));
        }
        assert!(
            lines.held() <= HELD_CAP,
            "held {} at line {top}",
            lines.held()
        );
    }
    assert!(lines.row(total - 1).is_some(), "the latest window is held");
    assert!(lines.row(1).is_none(), "the first window went first");
}

#[test]
fn code_page_step_lands_on_a_line_boundary() {
    assert_eq!(
        page_step(505., 20.),
        480.,
        "a page is whole lines only, one screen less a line"
    );
    assert_eq!(
        page_step(500., 20.),
        480.,
        "an aligned viewport keeps its last line on top as context"
    );
    assert_eq!(
        page_step(39., 20.),
        20.,
        "a short viewport still pages one line"
    );
    assert_eq!(
        page_step(0., 20.),
        20.,
        "a page before layout moves one line"
    );
    assert_eq!(
        page_step(503., 16.) % 16.,
        0.,
        "the page-forward offset is a whole multiple of the line height"
    );
    assert_eq!(
        page_step(100.75, 0.75),
        99.75,
        "a line under one pixel still pages by whole lines"
    );
    assert_eq!(
        page_step(300., 0.),
        300.,
        "no line height yet pages by the viewport"
    );
}

#[test]
fn code_limit_and_plain_lines() {
    let mut lines = Lines::new(1);
    lines.active = true;
    lines.limit = 2;
    lines.set_subject("/w/a.txt", true, Some("a\nb\nc\n".into()));
    assert_eq!(
        (lines.total, lines.shown()),
        (3, 2),
        "the limit caps the lines drawn, never the count"
    );
    assert_eq!(lines.plain_lines(), vec!["a", "b"]);
    assert_eq!(
        Answer::parse("nonsense"),
        Answer::Failed,
        "an unknown state is a failed read, never a partial one"
    );
}
