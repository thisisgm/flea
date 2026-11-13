//! Grid tiles draw language marks and thumbnails through the production lender, as v0.3.12's GridTile.qml does.
use super::pictures::Pictures;
use crate::{
    backend::Backend,
    grid::join_tests::{frame, named_shell},
    list::pane::Held,
    state::Shell,
    window::startup::View,
};
use flea::protocol::RowFacts;
use gpui::{TestAppContext, WindowHandle};
use std::{rc::Rc, time::Duration};

const PICTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/images/transfer.png"
);
const LOCAL: &str = r#"{"t":"fsinfo","path":"/files","class":""}"#;
const FSINFO: &str = r#"{"c":"fsinfo"}"#;

// The Media fixture's first rows, named the way the native grid scene names them.
fn named(row: &mut RowFacts, at: usize) {
    let (name, icon, dir, mode) = match at {
        0 => ("main.rs", "text-x-generic", false, 0o100644),
        1 => ("photo.png", "image-x-generic", false, 0o100644),
        2 => ("notes.txt", "text-plain", false, 0o100644),
        3 => ("lib.rs", "folder", true, 0o040755),
        4 => ("link.rs", "text-x-generic", false, 0o120777),
        5 => ("clip.mp4", "video-x-generic", false, 0o100644),
        6 => ("Makefile", "text-x-generic", false, 0o100644),
        _ => return,
    };
    row.name = name.into();
    row.icon = icon.into();
    row.dir = dir;
    row.mode = mode;
    row.size = 100 + at as u64;
    row.mtime = 1_700_000_000;
}

fn open(
    cx: &mut TestAppContext,
    settings: &str,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<String>) {
    let (backend, lines) = Backend::test_lines();
    let settings = flea::jsondoc::parse(settings).unwrap();
    let handle = cx.add_window(|_, cx| {
        let mut shell = named_shell(backend, cx);
        shell.launch.settings.data = settings;
        let held = &mut shell.panes[0].as_mut().unwrap().pane.held;
        for (at, row) in Rc::get_mut(held).unwrap().rows.iter_mut().enumerate() {
            named(row, at);
        }
        shell
    });
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    (handle, lines)
}

// One frame as production runs it, where render.rs syncs the pictures every frame: settle, sync, draw.
fn step(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| shell.sync_grid_pictures(window, cx))
        .unwrap();
    frame(cx, handle);
}

fn deliver(cx: &mut TestAppContext, handle: WindowHandle<Shell>, line: &str) -> bool {
    let event = Backend::test_reply_line(line);
    handle
        .update(cx, |shell, window, cx| {
            shell.grid_pictures_event(0, &event, window, cx)
        })
        .unwrap()
}

fn settle(cx: &mut TestAppContext, handle: WindowHandle<Shell>, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
    step(cx, handle);
}

fn drained(lines: &mut impl FnMut() -> Option<String>) -> Vec<String> {
    std::iter::from_fn(lines).collect()
}

fn read<R>(cx: &mut TestAppContext, handle: WindowHandle<Shell>, f: impl FnOnce(&Shell) -> R) -> R {
    handle.update(cx, |shell, _, _| f(shell)).unwrap()
}

fn pictures(shell: &Shell) -> &Pictures {
    let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
    grid.decor
        .pictures()
        .expect("the shown Grid lends the production pictures")
}

// What the last frame drew for one row: its language mark key and its decoded picture size.
fn drawn(shell: &Shell, row: usize) -> (Option<String>, Option<(u32, u32)>) {
    let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
    let state = grid.state.borrow();
    let tile = state.drawn_row(row).expect("the row is a drawn tile");
    (tile.language_key().map(str::to_owned), tile.decoded)
}

fn thumbed(row: usize, file: &str) -> String {
    format!(r#"{{"t":"thumbed","row":{row},"file":"{file}","ms":3}}"#)
}

// The first settle in a listing, after fsinfo names an ordinary local class.
fn first_settle(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    lines: &mut impl FnMut() -> Option<String>,
) -> Vec<String> {
    step(cx, handle);
    assert!(
        !deliver(cx, handle, LOCAL),
        "fsinfo stays with its other readers"
    );
    step(cx, handle);
    settle(cx, handle, 70);
    drained(lines)
}

#[gpui::test]
fn code_files_draw_their_language_mark_in_place_of_the_kind_mark(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx, "{}");
    step(cx, handle);
    let marks = read(cx, handle, |shell| {
        (0..7).map(|row| drawn(shell, row).0).collect::<Vec<_>>()
    });
    assert_eq!(
        marks[0].as_deref(),
        Some("flea-lang:rust"),
        "main.rs draws the Rust mark"
    );
    assert_eq!(
        marks[6].as_deref(),
        Some("flea-lang:gnu"),
        "a Makefile is matched by name"
    );
    for row in [1, 2, 5] {
        assert_eq!(
            marks[row], None,
            "{row}: a file with no language keeps its kind mark"
        );
    }
    assert_eq!(marks[3], None, "a folder named like code stays a folder");
    assert_eq!(marks[4], None, "a symlink keeps its link mark");
}

#[gpui::test]
fn code_icons_turned_off_draw_kind_marks_only(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx, r#"{"langMarks":false}"#);
    step(cx, handle);
    assert_eq!(read(cx, handle, |shell| drawn(shell, 0).0), None);
}

#[gpui::test]
fn visible_files_are_asked_once_after_the_first_settle_and_cancelled_when_scrolled_away(
    cx: &mut TestAppContext,
) {
    let (handle, mut lines) = open(cx, "{}");
    step(cx, handle);
    drained(&mut lines);
    deliver(cx, handle, LOCAL);
    step(cx, handle);
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.run_until_parked();
    assert!(
        drained(&mut lines)
            .iter()
            .all(|line| !line.contains("\"thumb\"")),
        "nothing is asked before the 70 ms settle"
    );
    settle(cx, handle, 10);
    let asked = drained(&mut lines);
    let thumb = asked
        .iter()
        .find(|line| line.starts_with(r#"{"c":"thumb","#))
        .expect("one thumb line");
    assert!(
        thumb.starts_with(r#"{"c":"thumb","rows":[0,1,2,4,5,6,"#),
        "every visible file, never the folder: {thumb}"
    );
    assert!(thumb.ends_with(r#"],"cacheOnly":false}"#), "{thumb}");
    assert_eq!(
        thumb_requests(cx, handle),
        "1",
        "one thumb line, one request"
    );
    let waiting = read(cx, handle, |shell| pictures(shell).waiting());
    settle(cx, handle, 200);
    assert!(
        drained(&mut lines).is_empty(),
        "a still frame asks nothing again"
    );
    handle
        .update(cx, |shell, _, _| {
            let grid = shell.panes[0].as_mut().unwrap().grid.as_mut().unwrap();
            grid.view.scroll = 2_000.;
        })
        .unwrap();
    step(cx, handle);
    settle(cx, handle, 120);
    let moved = drained(&mut lines);
    let cancel = moved
        .iter()
        .find(|line| line.starts_with(r#"{"c":"thumbcancel","#))
        .expect("a cancel for rows left behind");
    assert!(cancel.contains(&format!("[{}", waiting[0])), "{cancel}");
    assert!(
        moved
            .iter()
            .any(|line| line.starts_with(r#"{"c":"thumb","#)),
        "the rows now in view are asked"
    );
    assert_eq!(
        thumb_requests(cx, handle),
        "2",
        "a thumbcancel is not a thumb request"
    );
}

// Backend.qml thumbRequests through the window's IPC dispatch.
fn thumb_requests(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> String {
    handle
        .update(cx, |shell, window, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            let request = flea::ipc::wire::Request::Call {
                target: "flea".into(),
                function: "thumbRequests".into(),
                arguments: vec![],
            };
            shell.ipc(
                crate::ipc::server::Event::Call { request, answer },
                window,
                cx,
            );
            let reply = receive.try_recv().unwrap().reply;
            assert_eq!(reply.code, 0, "{}", reply.stderr);
            reply.stdout.trim().to_owned()
        })
        .unwrap()
}

#[gpui::test]
fn a_thumbed_file_is_decoded_off_the_ui_thread_and_replaces_the_mark(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, "{}");
    first_settle(cx, handle, &mut lines);
    assert!(
        deliver(cx, handle, &thumbed(0, PICTURE)),
        "a thumbed line is the Grid's"
    );
    step(cx, handle);
    assert_eq!(
        read(cx, handle, |shell| pictures(shell).lent()),
        0,
        "the UI thread never decodes"
    );
    assert_eq!(read(cx, handle, |shell| drawn(shell, 0).1), None);
    cx.run_until_parked();
    step(cx, handle);
    let (mark, decoded) = read(cx, handle, |shell| drawn(shell, 0));
    assert_eq!(
        decoded,
        Some((1280, 64)),
        "the decoded picture draws on its tile"
    );
    assert_eq!(mark, None, "a drawn picture replaces the language mark");
    let counters = read(cx, handle, |shell| pictures(shell).counters());
    assert_eq!(
        (counters.answered, counters.decoded, counters.refused),
        (1, 1, 0)
    );
}

#[gpui::test]
fn refused_missing_and_unasked_answers_draw_no_picture(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, "{}");
    first_settle(cx, handle, &mut lines);
    deliver(cx, handle, &thumbed(1, ""));
    deliver(cx, handle, &thumbed(2, "/nonexistent/flea-thumb.png"));
    deliver(cx, handle, &thumbed(3, PICTURE));
    cx.run_until_parked();
    step(cx, handle);
    for row in 1..4 {
        assert_eq!(read(cx, handle, |shell| drawn(shell, row).1), None, "{row}");
    }
    let counters = read(cx, handle, |shell| pictures(shell).counters());
    assert_eq!(
        (counters.answered, counters.decoded, counters.refused),
        (2, 0, 2),
        "the folder was never asked"
    );
}

#[gpui::test]
fn a_row_whose_facts_changed_never_draws_the_old_picture_and_is_asked_again(
    cx: &mut TestAppContext,
) {
    let (handle, mut lines) = open(cx, "{}");
    first_settle(cx, handle, &mut lines);
    deliver(cx, handle, &thumbed(1, PICTURE));
    cx.run_until_parked();
    step(cx, handle);
    assert!(read(cx, handle, |shell| drawn(shell, 1).1).is_some());
    handle
        .update(cx, |shell, _, _| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            let mut rows = pane.held.rows.clone();
            rows[1].mtime += 60;
            pane.held = Rc::new(Held { rows, ..*pane.held });
        })
        .unwrap();
    step(cx, handle);
    assert_eq!(
        read(cx, handle, |shell| drawn(shell, 1).1),
        None,
        "an edited file never shows its old picture"
    );
    settle(cx, handle, 120);
    assert!(drained(&mut lines)
        .iter()
        .any(|line| line == r#"{"c":"thumb","rows":[1],"cacheOnly":false}"#));
}

#[path = "centering_tests.rs"]
mod centering;

#[path = "pictures_relist_tests.rs"]
mod relist;

#[path = "pictures_query_tests.rs"]
mod query;

// ui.sh nosweep: a folder of text files sends no thumb line in either view, as v0.3.12 Thumbs.allowed reads "t":false.
#[gpui::test]
fn a_folder_of_text_files_sends_no_thumb_request_in_list_or_grid(cx: &mut TestAppContext) {
    for view in [View::List, View::Grid] {
        let (backend, mut lines) = Backend::test_lines();
        let handle = cx.add_window(|_, cx| {
            let mut shell = named_shell(backend, cx);
            let held = &mut shell.panes[0].as_mut().unwrap().pane.held;
            for row in &mut Rc::get_mut(held).unwrap().rows {
                row.thumbless = true;
            }
            shell
        });
        handle
            .update(cx, |shell, window, cx| shell.set_view(view, window, cx))
            .unwrap();
        first_settle(cx, handle, &mut lines);
        settle(cx, handle, 200);
        let sent = drained(&mut lines);
        assert!(
            sent.iter()
                .all(|line| !line.starts_with(r#"{"c":"thumb","#)),
            "{view:?}: {sent:?}"
        );
        assert_eq!(thumb_requests(cx, handle), "0", "{view:?}");
    }
}
