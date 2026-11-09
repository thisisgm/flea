//! Mounted join: a presented submission of the search's own rows stamps its generation, an older scene never does.
use crate::{
    backend::{Backend, Event as BackendEvent},
    g2::file_search::SearchAdapter,
    ipc::Adapter,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use flea::{
    jsondoc::{self, Json},
    protocol::{Reply, RowFacts},
};
use gpui::{
    AppContext, Context, Entity, FrameOutcome, FrameTerminal, KeyDownEvent, Keystroke,
    PresentationTime, RequestFrameOptions, TestAppContext, VisualTestContext, Window,
};
use std::time::Duration;

fn mount(
    cx: &mut TestAppContext,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    impl FnMut() -> Option<String>,
) {
    let (backend, requests) = Backend::test_lines();
    let (root, cx) = cx.add_window_view(|_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    cx.enable_frame_feedback();
    edit(&shell, cx, |shell, window, cx| {
        cx.set_reduce_motion(true);
        shell.follow_frame_feedback(window, cx);
        window.focus(&shell.focus, cx);
        cx.notify();
    });
    (shell, cx, requests)
}

fn edit<R>(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    cx.update(|window, cx| shell.update(cx, |shell, cx| change(shell, window, cx)))
}

fn key(shell: &Entity<Shell>, cx: &mut VisualTestContext, text: &str) {
    edit(shell, cx, |shell, window, cx| {
        let keystroke = Keystroke {
            key: text.into(),
            key_char: Some(text.into()),
            ..Default::default()
        };
        let event = KeyDownEvent {
            keystroke,
            is_held: false,
            prefer_character_input: false,
        };
        shell.key(&event, window, cx)
    });
    cx.run_until_parked();
}

fn answer(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    generation: usize,
    name: &str,
    walking: bool,
) {
    edit(shell, cx, |shell, window, cx| {
        let listed = format!(r#"{{"t":"listed","path":"/files","n":1,"gen":{generation}}}"#);
        shell.receive(0, Backend::test_reply_line(&listed), window, cx);
        let rows = vec![RowFacts {
            name: name.into(),
            icon: "text-plain".into(),
            ..Default::default()
        }];
        let reply = Reply::Rows {
            start: 0,
            listing: 10 + generation,
            rows,
        };
        shell.receive(0, BackendEvent::Session { generation, reply }, window, cx);
        let searched = format!(
            r#"{{"t":"searched","n":1,"scanned":11,"ms":1,"cancelled":false,"gen":{generation},"walking":{walking}}}"#
        );
        shell.receive(0, Backend::test_reply_line(&searched), window, cx);
    });
    cx.run_until_parked();
}

// One real Window submission and its compositor terminal; None presents nothing.
fn frame(cx: &mut VisualTestContext, presented_ns: Option<u64>) -> u64 {
    cx.update(|window, _| window.refresh());
    cx.simulate_frame_request(RequestFrameOptions {
        require_presentation: true,
        ..Default::default()
    });
    cx.run_until_parked();
    let id = cx.update(|window, _| window.presented_frames().unwrap().committed);
    let outcome = presented_ns.map_or(FrameOutcome::Discarded, |ns| {
        FrameOutcome::Presented(PresentationTime {
            timestamp: Some(Duration::from_nanos(ns)),
            clock_id: Some(1),
        })
    });
    cx.simulate_frame_terminal(FrameTerminal {
        frame_id: id,
        outcome,
    });
    cx.run_until_parked();
    id
}

fn stamps(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    shell.read_with(cx, |shell, _| {
        let search = &shell.panes[0].as_ref().unwrap().search.owner;
        let reply = SearchAdapter { search }.call("searchStamps", &[]).unwrap();
        jsondoc::parse(reply.stdout.trim()).unwrap()
    })
}

fn presented(stamps: &Json, key: &str) -> Option<(u64, String)> {
    let frame = stamps.get(key)?;
    let number = |field| {
        frame
            .get(field)
            .and_then(Json::as_f64)
            .map(|value| value as u64)
    };
    Some((
        number("frame")?,
        format!("{:?}/{:?}", number("ns"), number("clock")),
    ))
}

#[gpui::test]
fn the_first_presented_frame_of_the_rows_stamps_the_generation(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mount(cx);
    frame(cx, Some(1_000));
    key(&shell, cx, "f");
    key(&shell, cx, "a");
    answer(&shell, cx, 1, "Docs/a.txt", false);
    let discarded = frame(cx, None);
    assert_eq!(
        presented(&stamps(&shell, cx), "presentedFirst"),
        None,
        "a discarded frame {discarded} showed nothing"
    );
    let shown = frame(cx, Some(5_000_000));
    let now = stamps(&shell, cx);
    let expected = Some((shown, "Some(5000000)/Some(1)".to_owned()));
    assert_eq!(presented(&now, "presentedFirst"), expected, "{now:?}");
    assert_eq!(
        presented(&now, "presentedSettled"),
        expected,
        "the rows were painted after the final answer"
    );
    assert_eq!(
        now.get("firstName").and_then(Json::as_str),
        Some("Docs/a.txt")
    );
    assert_eq!(now.get("firstRow").and_then(Json::as_f64), Some(0.));
    assert_eq!(now.get("indexed").and_then(Json::as_f64), Some(11.));
    assert_eq!(now.get("coverage").and_then(Json::as_str), Some("complete"));
    frame(cx, Some(6_000_000));
    assert_eq!(
        presented(&stamps(&shell, cx), "presentedFirst"),
        expected,
        "a later frame never moves the first"
    );
}

#[gpui::test]
fn held_rows_of_an_older_query_never_stamp_the_newer_one(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mount(cx);
    frame(cx, Some(1_000));
    key(&shell, cx, "f");
    key(&shell, cx, "a");
    answer(&shell, cx, 1, "Docs/a.txt", false);
    frame(cx, Some(2_000));
    key(&shell, cx, "b");
    frame(cx, Some(3_000));
    let typed = stamps(&shell, cx);
    assert_eq!(typed.get("gen").and_then(Json::as_f64), Some(2.));
    assert_eq!(
        presented(&typed, "presentedFirst"),
        None,
        "gen 1's rows are still drawn: {typed:?}"
    );
    answer(&shell, cx, 2, "Docs/ab.txt", true);
    let walking = frame(cx, Some(4_000));
    let now = stamps(&shell, cx);
    assert_eq!(
        presented(&now, "presentedFirst").map(|(id, _)| id),
        Some(walking)
    );
    assert_eq!(
        presented(&now, "presentedSettled"),
        None,
        "a walking answer has not settled"
    );
    edit(&shell, cx, |shell, window, cx| {
        let line = r#"{"t":"searched","n":1,"scanned":12,"ms":1,"cancelled":false,"gen":2,"walking":false}"#;
        shell.receive(0, Backend::test_reply_line(line), window, cx);
    });
    let settled = frame(cx, Some(5_000));
    let now = stamps(&shell, cx);
    assert_eq!(
        presented(&now, "presentedSettled").map(|(id, _)| id),
        Some(settled),
        "{now:?}"
    );
    assert_eq!(
        presented(&now, "presentedFirst").map(|(id, _)| id),
        Some(walking)
    );
    assert_eq!(
        now.get("firstName").and_then(Json::as_str),
        Some("Docs/ab.txt")
    );
}

#[gpui::test]
fn a_submission_lost_before_its_terminal_marks_the_coverage_lost(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mount(cx);
    frame(cx, Some(1_000));
    key(&shell, cx, "f");
    key(&shell, cx, "a");
    answer(&shell, cx, 1, "Docs/a.txt", false);
    // The first submission of the rows never gets its terminal before the feedback deadline retires it.
    cx.update(|window, _| window.refresh());
    cx.simulate_frame_request(RequestFrameOptions {
        require_presentation: true,
        ..Default::default()
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(crate::visual::FRAME_WAIT + Duration::from_millis(100));
    cx.run_until_parked();
    let later = frame(cx, Some(9_000_000));
    let now = stamps(&shell, cx);
    assert_eq!(
        presented(&now, "presentedFirst").map(|(id, _)| id),
        Some(later)
    );
    assert_eq!(
        now.get("coverage").and_then(Json::as_str),
        Some("lost"),
        "the later frame is not provably first: {now:?}"
    );
}

#[gpui::test]
fn an_earlier_submission_without_its_terminal_keeps_the_first_pending(cx: &mut TestAppContext) {
    let (shell, cx, _requests) = mount(cx);
    frame(cx, Some(1_000));
    key(&shell, cx, "f");
    key(&shell, cx, "a");
    answer(&shell, cx, 1, "Docs/a.txt", false);
    cx.update(|window, _| window.refresh());
    cx.simulate_frame_request(RequestFrameOptions {
        require_presentation: true,
        ..Default::default()
    });
    cx.run_until_parked();
    let early = cx.update(|window, _| window.presented_frames().unwrap().committed);
    let later = frame(cx, Some(9_000_000));
    let now = stamps(&shell, cx);
    assert_eq!(
        presented(&now, "presentedFirst").map(|(id, _)| id),
        Some(later)
    );
    assert_eq!(
        now.get("coverage").and_then(Json::as_str),
        Some("pending"),
        "{now:?}"
    );
    let time = PresentationTime {
        timestamp: Some(Duration::from_nanos(8_000_000)),
        clock_id: Some(1),
    };
    cx.simulate_frame_terminal(FrameTerminal {
        frame_id: early,
        outcome: FrameOutcome::Presented(time),
    });
    cx.run_until_parked();
    let now = stamps(&shell, cx);
    assert_eq!(
        presented(&now, "presentedFirst").map(|(id, _)| id),
        Some(early),
        "{now:?}"
    );
    assert_eq!(now.get("coverage").and_then(Json::as_str), Some("complete"));
}
