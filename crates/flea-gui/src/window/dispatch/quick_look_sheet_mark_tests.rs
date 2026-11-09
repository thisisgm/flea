//! A sheet turn holds the reading mark like a new subject and clears it on the frame the sheet's first screen draws.
use super::super::preview_tests::{answer, body, close, draw, opened, sent, token, Lines};
use super::super::test_shell::land;
use super::{open_workbook, rows, sheet};
use crate::{
    backend::Event as BackendEvent,
    g2::quick_look_overlay::{doc::View, Body, Hit},
    pane::swap::HOLD_MS,
    state::Shell,
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

// The reply to the sheet table's code ask in flight, carrying `lines`.
fn sheet_reply(requests: &mut Lines, lines: &[&str]) -> String {
    let ask = sent(requests, "code");
    let token = token(ask.last().expect("the sheet asks for its lines"));
    let body: Vec<String> = lines.iter().map(|l| format!(r#"[0,"{l}"]"#)).collect();
    format!(
        r#"{{"t":"code","token":{token},"slot":"look","state":"plain","first":1,"total":{n},"records":{n},"firstRecord":1,"lines":[{}]}}"#,
        body.join(","),
        n = lines.len()
    )
}

fn answer_sheet(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    requests: &mut Lines,
    lines: &[&str],
) {
    let reply = sheet_reply(requests, lines);
    answer(shell, cx, reply);
}

// Only the frame the parse's own notify draws, as on a real display, where nothing redraws after it.
fn answer_sheet_one_frame(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    requests: &mut Lines,
    lines: &[&str],
) {
    let reply = sheet_reply(requests, lines);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, BackendEvent::Producer(reply), window, cx)
        })
    });
    cx.run_until_parked();
}

// The sheet table's drawn body rows in the last frame.
fn drawn_rows(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> usize {
    match body(shell, cx) {
        Some(Body::Doc(doc)) => match &doc.view {
            View::Office(office) => cx.read(|cx| office.read(cx).table.read(cx).rows.len()),
            _ => panic!("a workbook mounts the office view"),
        },
        other => panic!("no mounted view: {other:?}"),
    }
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    draw(cx);
}

fn mark(cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds("pv1-waiting").is_some()
}

// S12C-3: ] and a ring word turn the sheet; no mark inside the hold, the mark while the sheet is out, none once it draws.
#[gpui::test]
fn a_turned_sheet_clears_the_reading_mark_on_the_frame_its_rows_draw(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    open_workbook(&shell, cx, &mut requests, 1, &["First", "Second"]);
    answer_sheet(&shell, cx, &mut requests, &["a,b", "1,2"]);
    wait(cx, HOLD_MS + 50);
    assert_eq!(sheet(&shell, cx), (0, 2));
    assert!(drawn_rows(&shell, cx) > 0, "First's rows draw");
    assert!(!mark(cx), "a drawn sheet draws no mark");

    cx.simulate_keystrokes("]");
    draw(cx);
    assert_eq!(sheet(&shell, cx), (1, 2), "] turns to Second");
    assert!(!mark(cx), "nothing flashes inside the turn's hold");
    wait(cx, HOLD_MS + 50);
    assert!(mark(cx), "a sheet still out past the hold draws the mark");
    answer_sheet_one_frame(&shell, cx, &mut requests, &["x,y", "3,4", "5,6"]);
    assert!(drawn_rows(&shell, cx) > 0, "Second's rows draw");
    assert!(
        !mark(cx),
        "the frame that draws Second's rows draws no mark"
    );

    let look = shell.read_with(cx, |shell, _| {
        shell.quick_look.as_ref().unwrap().view.clone()
    });
    cx.update(|_, cx| look.update(cx, |look, cx| look.activate(Hit::Word(0), cx)));
    draw(cx);
    assert_eq!(sheet(&shell, cx), (0, 2), "the ring word First turns back");
    assert!(!mark(cx), "nothing flashes inside the word's hold");
    answer_sheet(&shell, cx, &mut requests, &["a,b", "1,2"]);
    assert!(drawn_rows(&shell, cx) > 0, "First's rows draw again");
    wait(cx, HOLD_MS + 50);
    assert!(
        !mark(cx),
        "a sheet that drew inside the hold never draws the mark"
    );
    close(&shell, cx);
}
