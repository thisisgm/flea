//! tests/trash-view.qml through the GPUI pane, the trash.js wire pins as Rust source pins, and the monitor service.
use super::monitor::{Heard, Monitor};
use super::seams::At;
use super::tests::{answer, listed, opened, row, sent, Call};
use crate::list::bench::real_cx;
use std::time::{Duration, Instant};

fn menus(calls: &[Call]) -> Vec<(Vec<&'static str>, At)> {
    calls
        .iter()
        .filter_map(|call| match call {
            Call::Menu(ids, at) => Some((ids.clone(), *at)),
            _ => None,
        })
        .collect()
}

#[test]
fn trash_view_through_the_pane() {
    let mut cx = real_cx();
    let rows = [
        row("photo.png", None),
        row("notes.txt", None),
        row("old-folder", None),
    ];
    let (window, calls) = opened(&mut cx, &rows);
    window
        .update(&mut cx, |pane, _, cx| {
            let paint = pane.paint.borrow();
            let names: Vec<&str> = (0..3)
                .map(|i| paint.line(i).unwrap().name.as_str())
                .collect();
            assert_eq!(
                names,
                ["photo.png", "notes.txt", "old-folder"],
                "rows keep the order the backend gives"
            );
            drop(paint);
            assert_eq!(
                pane.header_labels(),
                ["Name", "Original Location", "Deleted"],
                "the header labels stay bare, in the board's title case"
            );
            assert_eq!(
                crate::navigation::sort::title(&pane.header_labels()[2], true),
                "Deleted \u{25be}",
                "the Deleted header draws the descending mark"
            );
            pane.action("menu", false, cx);
        })
        .unwrap();
    let opened_menus = menus(&calls.borrow());
    assert_eq!(
        opened_menus,
        [(vec!["restoreTrashSelection"], At { x: 14., y: 88. })],
        "m builds the menu once, Restore alone, under the cursor row"
    );
    window
        .update(&mut cx, |pane, _, cx| {
            pane.menu_action(Some("restoreTrashSelection"), cx)
        })
        .unwrap();
    let restore = sent(&calls)
        .into_iter()
        .rev()
        .find(|line| line.contains(r#""op":"restore""#))
        .unwrap();
    assert!(
        restore.contains(r#""all":false"#) && restore.contains(r#""uris":["trash:///photo.png"]"#),
        "Restore restores the newest item, not the whole Trash"
    );
    answer(
        &mut cx,
        window,
        &calls,
        "restore",
        r#","done":1,"failed":0,"failures":[]"#,
    );
    answer(&mut cx, window, &calls, "list", &listed(&rows[1..]));
    window
        .update(&mut cx, |pane, _, _| {
            assert_eq!(
                pane.paint.borrow().line(0).unwrap().name,
                "photo.png",
                "the previous complete snapshot remains until the summary joins its rows"
            );
        })
        .unwrap();
    answer(
        &mut cx,
        window,
        &calls,
        "summary",
        r#","bytes":26,"partial":false"#,
    );

    window
        .update(&mut cx, |pane, _, cx| {
            assert_eq!(
                pane.paint.borrow().line(0).unwrap().name,
                "notes.txt",
                "the restored row is gone and the next newest leads"
            );
            assert!(
                pane.view().selection.uris().is_empty(),
                "nothing is marked after a restore"
            );
            pane.action("menu", false, cx);
            assert_eq!(
                pane.view().selection.uris(),
                ["trash:///notes.txt"],
                "having marked that row for the action"
            );
            pane.menu_action(None, cx);
            pane.action("extendDown", false, cx);
            pane.action("menu", false, cx);
        })
        .unwrap();
    let all = menus(&calls.borrow());
    assert_eq!(
        all[1],
        (vec!["restoreTrashSelection"], At { x: 14., y: 88. }),
        "m with nothing marked opens under the cursor row"
    );
    assert_eq!(
        all[2],
        (vec!["restoreTrashSelection"], At { x: 454., y: 27. }),
        "two marked rows open at the top centre, under the strip"
    );
    assert_eq!(all.len(), 3, "each m builds one menu");
}

#[test]
fn trash_wire_pins_hold_in_the_rust_sources() {
    let ops = include_str!("../../../../../src/ui_logic_pl3/view_ops.rs");
    assert!(
        ops.contains("words::route(action) != Route::Trash"),
        "Trash forwards by route instead of a list of names"
    );
    let reply = include_str!("../../../../../src/ui_logic_pl3/view_reply.rs");
    assert!(
        reply.contains("if self.landing && self.total > 0 {")
            && ops.contains(
                "words::menu_target(self.total, self.selected_count(), self.is_selected(&row.uri))"
            ),
        "the view lands on the newest row and m asks the target rule"
    );
    let pane = include_str!("pane.rs");
    assert!(
        pane.contains("words::confirm_title(&self.catalog, &snapshot)")
            && pane.contains("words::confirm_body(&self.catalog, &snapshot, scope)"),
        "the card draws its words from the unfilled guard"
    );
    let strip = include_str!("strip.rs");
    // The pin reads through the formatter: one blank stands for every run of whitespace.
    let flat = strip.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains(r#"sort::title( &self.catalog.tr("trash.header.deleted"), true,"#),
        "the header draws the shared descending mark"
    );
    assert_eq!(
        format!(
            "{}|{}",
            crate::navigation::sort::title("Modified", true),
            crate::navigation::sort::title("Modified", false)
        ),
        "Modified \u{25be}|Modified \u{25b4}",
        "the shared mark is the List view's pair"
    );
}

fn heard(receiver: &async_channel::Receiver<Heard>, want: usize, wait: Duration) -> Vec<Heard> {
    let deadline = Instant::now() + wait;
    let mut got = Vec::new();
    while got.len() < want && Instant::now() < deadline {
        match receiver.try_recv() {
            Ok(message) => got.push(message),
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    got
}

fn sh(script: &str) -> Vec<String> {
    vec!["sh".into(), "-c".into(), script.into()]
}

#[test]
fn the_monitor_reads_a_settled_count_per_burst_and_reports_failures() {
    let long = Duration::from_secs(10);
    let (monitor, receiver) = Monitor::start_with(
        sh("sleep 0.5; echo a; echo b; sleep 0.5; echo c; sleep 5"),
        sh("echo '  trash::item-count: 3'"),
    )
    .unwrap();
    assert_eq!(
        heard(&receiver, 5, long),
        [
            Heard::Count(3),
            Heard::Changed,
            Heard::Count(3),
            Heard::Changed,
            Heard::Count(3)
        ],
        "the start reads once, and each settled burst says it changed and reads once"
    );
    assert!(
        heard(&receiver, 1, Duration::from_millis(700)).is_empty(),
        "a quiet watch reads nothing more"
    );
    drop(monitor);
    let (_monitor, receiver) = Monitor::start_with(sh("sleep 5"), sh("exit 1")).unwrap();
    assert_eq!(
        heard(&receiver, 1, long),
        [Heard::Failed("trash.monitor.count_failed")],
        "a failed read keeps the count and says so"
    );
    let (_monitor, receiver) =
        Monitor::start_with(sh("true"), sh("echo 'trash::item-count: 1'")).unwrap();
    let got = heard(&receiver, 2, Duration::from_secs(3));
    assert_eq!(
        got.last(),
        Some(&Heard::Failed("trash.monitor.watch_failed")),
        "a watch that ends says the count may be out of date"
    );
}
