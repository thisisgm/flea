//! Soft re-list deferral across Trash scroll, and the settled-burst path that arms it.
use super::monitor::{Heard, Monitor, Started};
use super::tests::{answer, listed, open, opened, row, sent, Call};
use crate::list::bench::real_cx;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

type Calls = Rc<RefCell<Vec<Call>>>;

fn burst() -> Started {
    let sh = |script: &str| vec!["sh".to_string(), "-c".to_string(), script.to_string()];
    Monitor::start_with(
        sh("sleep 0.3; echo change; sleep 5"),
        sh("echo 'trash::item-count: 1'"),
    )
}

#[test]
fn an_open_trash_starts_its_watch_and_re_reads_after_each_settled_burst() {
    let mut cx = real_cx();
    let (window, calls) = open(&mut cx);
    window
        .update(&mut cx, |pane, _, cx| {
            pane.monitor_start = Some(burst);
            pane.open("", cx);
        })
        .unwrap();
    answer(
        &mut cx,
        window,
        &calls,
        "list",
        &listed(&[row("a.txt", None)]),
    );
    answer(
        &mut cx,
        window,
        &calls,
        "summary",
        r#","bytes":13,"partial":false"#,
    );
    let lists = |calls: &Calls| {
        sent(calls)
            .iter()
            .filter(|line| line.contains(r#""op":"list""#))
            .count()
    };
    // First settle is Count-only; the scripted change settles with Changed and soft-arms.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut armed = false;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        cx.run_until_parked();
        armed = window
            .update(&mut cx, |pane, _, _| pane.soft_pending_for_test())
            .unwrap();
        if armed {
            break;
        }
    }
    assert!(armed, "the settled Changed burst must soft-arm a re-list");
    assert!(
        calls.borrow().contains(&Call::Count(1)),
        "the watch's count reaches the rail"
    );
    cx.executor().advance_clock(Duration::from_millis(
        flea::ui_logic_pl3::monitor::SOFT_RELIST_MS + 100,
    ));
    cx.run_until_parked();
    assert_eq!(
        lists(&calls),
        2,
        "the settled burst soft re-lists the open listing once idle"
    );
}

#[test]
fn soft_relist_runs_once_scrolling_is_idle() {
    let mut cx = real_cx();
    let (window, calls) = opened(&mut cx, &[row("a.txt", None), row("b.txt", None)]);
    let before = sent(&calls).len();
    window
        .update(&mut cx, |pane, _, cx| {
            pane.heard(Heard::Changed, cx);
            pane.wheel(40., cx);
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    let after = sent(&calls);
    assert!(
        after[before..]
            .iter()
            .any(|line| line.contains(r#""op":"list""#)),
        "Changed then scroll then idle must soft re-list: {:?}",
        &after[before..]
    );
}

#[test]
fn soft_relist_waits_out_continuous_scroll() {
    let mut cx = real_cx();
    let (window, calls) = opened(&mut cx, &[row("a.txt", None), row("b.txt", None)]);
    let before = sent(&calls).len();
    window
        .update(&mut cx, |pane, _, cx| {
            pane.heard(Heard::Changed, cx);
            pane.wheel(40., cx);
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    window
        .update(&mut cx, |pane, _, cx| pane.wheel(40., cx))
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    let mid = sent(&calls);
    assert!(
        mid[before..]
            .iter()
            .all(|line| !line.contains(r#""op":"list""#)),
        "soft re-list must not fire while scrolling continues: {:?}",
        &mid[before..]
    );
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    let after = sent(&calls);
    assert!(
        after[before..]
            .iter()
            .any(|line| line.contains(r#""op":"list""#)),
        "soft re-list must fire once scrolling settles: {:?}",
        &after[before..]
    );
}
