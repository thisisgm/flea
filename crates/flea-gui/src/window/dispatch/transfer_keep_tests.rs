//! A finished transfer re-reads the shown folder in place: the cursor's file, the marks and the scroll stay.
use super::super::test_shell::{asked_on, open_in, row};
use crate::{
    backend::{Backend, Event as BackendEvent, Located},
    navigation::{anchor::Anchor, watch::WATCH_MS},
    state::Shell,
    window::startup::View,
};
use flea::protocol::{Reply, RowFacts};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

#[path = "reread_place_tests.rs"]
mod reread_place;

const DONE: &str = r#"{"t":"transferdone","id":7,"ok":1,"failed":0,"skipped":0,"cancelled":false}"#;
const TOTAL: usize = 1000;
// Enough answer rounds for a list and its window, an anchor's locate and one owed re-read after them.
const SERVE_ROUNDS: usize = 8;
// One watch deadline, the wait between a change notice and its re-read.
const WATCH: Duration = Duration::from_millis(WATCH_MS);

fn name(index: usize) -> String {
    format!("n{index:04}")
}

fn folder() -> Vec<RowFacts> {
    (0..TOTAL).map(|index| row(&name(index), false)).collect()
}

// The backend these tests stand in for: it answers the list out as Listing::first_start does, then every window the pane asks for and the anchor's locate of marked files outside the window.
fn serve(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    side: usize,
    path: &str,
    all: &[RowFacts],
) {
    for _ in 0..SERVE_ROUNDS {
        let replies = shell.read_with(cx, |shell, _| {
            let host = shell.panes[side].as_ref().unwrap();
            let window = |start: usize, count: usize, listing: usize| Reply::Rows {
                start,
                rows: all[start.min(all.len())..start.saturating_add(count).min(all.len())]
                    .to_vec(),
                listing,
            };
            if let Some(pending) = host.pane.swap.pending.as_ref() {
                if pending.total.is_none() {
                    let select = host
                        .nav
                        .pending_select
                        .clone()
                        .filter(|select| !select.is_empty())
                        .or_else(|| host.nav.anchor.as_ref().and_then(Anchor::target));
                    let at = select.and_then(|select| {
                        all.iter()
                            .position(|row| format!("{path}/{}", row.name) == select)
                    });
                    let first = pending.first;
                    let listing = host.pane.greatest_listing + 1;
                    let start = match at {
                        Some(at) if first > 0 && at >= first => {
                            (at - first / 2).min(all.len().saturating_sub(first))
                        }
                        _ => 0,
                    };
                    return vec![
                        Reply::Listed {
                            path: path.into(),
                            total: all.len(),
                        },
                        window(start, first, listing),
                    ];
                }
            }
            host.pane
                .requests
                .outstanding
                .map(|request| vec![window(request.start, request.count, request.listing)])
                .unwrap_or_default()
        });
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                for reply in replies {
                    shell.receive(side, BackendEvent::Reply(reply), window, cx);
                }
            })
        });
        let located = shell.read_with(cx, |shell, _| {
            let anchor = shell.panes[side].as_ref().unwrap().nav.anchor.as_ref()?;
            let id = anchor.locate.filter(|_| !anchor.locate_done)?;
            let matches = anchor.marks.iter().filter_map(|mark| {
                let name = mark.name.as_ref()?;
                let at = all.iter().position(|row| &row.name == name)?;
                Some((format!("{path}/{name}"), at))
            });
            Some(Located {
                directory: path.into(),
                path: String::new(),
                index: -1,
                id: Some(id),
                transfer: 0,
                ok: true,
                matches: matches.collect(),
            })
        });
        if let Some(located) = located {
            receive(shell, cx, side, BackendEvent::Located(located));
        }
        cx.executor().advance_clock(WATCH);
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[side].as_ref().unwrap();
        assert!(!host.pane.busy(), "the listing landed");
        assert!(host.pane.requests.outstanding.is_none(), "no window is out");
    });
}

fn receive(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize, event: BackendEvent) {
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.receive(side, event, window, cx)));
    cx.run_until_parked();
}

// What the user is looking at: the cursor's file, how far below the viewport's top it is drawn, and the marked files.
#[derive(Debug, PartialEq)]
struct Place {
    cursor: String,
    offset: f32,
    marked: Vec<String>,
}

fn place(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize) -> Place {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[side].as_ref().unwrap();
        let pane = &host.pane;
        let named = |index: usize| {
            pane.held
                .row(index)
                .map_or_else(|| format!("unheld row {index}"), |row| row.name.clone())
        };
        let offset = match host.grid.as_ref().filter(|grid| grid.view.visible) {
            Some(grid) => grid
                .state
                .borrow()
                .drawn_row(pane.cursor)
                .map_or(f32::NAN, |tile| tile.cell.y),
            None => pane.cursor as f32 * pane.row_height() - pane.scroll_y,
        };
        Place {
            cursor: named(pane.cursor),
            offset,
            marked: host.selected.iter().map(|&index| named(index)).collect(),
        }
    })
}

fn lists(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize) -> usize {
    shell.read_with(cx, |shell, _| {
        shell.panes[side].as_ref().unwrap().nav.counters.lists
    })
}

// The window deep in a long folder, clear of its end: the cursor two steps below the viewport's top, two files marked.
fn deep<'a>(
    cx: &'a mut TestAppContext,
    view: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext, Place) {
    let (shell, cx) = open_in(cx, "default", view);
    serve(&shell, cx, 0, "/files", &folder());
    cx.simulate_keystrokes("end");
    serve(&shell, cx, 0, "/files", &folder());
    cx.simulate_keystrokes("pageup pageup pageup pageup pageup pageup pageup pageup");
    serve(&shell, cx, 0, "/files", &folder());
    cx.simulate_keystrokes("down down");
    serve(&shell, cx, 0, "/files", &folder());
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let cursor = host.pane.cursor;
            assert!(cursor > TOTAL - 300 && cursor < TOTAL - 1, "{cursor}");
            if host.grid.as_ref().is_none_or(|grid| !grid.view.visible) {
                let last_drawn = (host.pane.scroll_y / host.pane.row_height()) as usize
                    + host.pane.visible_rows();
                assert!(last_drawn + 2 < TOTAL, "the view is clear of the end");
            }
            host.selected = vec![cursor - 3, cursor];
            host.input.selection.sync(&host.selected);
        })
    });
    let before = place(&shell, cx, 0);
    assert_eq!(before.marked.len(), 2);
    assert_eq!(before.marked[1], before.cursor);
    assert!(before.offset > 0., "the cursor is drawn below the top");
    let scrolled = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        match host.grid.as_ref().filter(|grid| grid.view.visible) {
            Some(grid) => grid.state.borrow().scroll,
            None => host.pane.scroll_y,
        }
    });
    assert!(scrolled > 1000., "the view is scrolled deep: {scrolled}");
    (shell, cx, before)
}

// One transfer finished in the pane, and the folder it re-reads now holds `after`.
fn finish(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize, after: &[RowFacts]) {
    let path = asked_on(shell, cx, side);
    receive(shell, cx, side, BackendEvent::Operations(DONE.into()));
    serve(shell, cx, side, &path, after);
}

fn without(names: &[String]) -> Vec<RowFacts> {
    let mut rows = folder();
    rows.retain(|row| !names.contains(&row.name));
    rows
}

fn keeps_its_place_after_a_move_out(cx: &mut TestAppContext, view: &str) {
    let (shell, cx, before) = deep(cx, view);
    // A file far above the cursor was moved away, as a drop onto a neighbour folder does.
    finish(&shell, cx, 0, &without(&[name(100)]));
    assert_eq!(
        place(&shell, cx, 0),
        before,
        "the finished transfer left the cursor's file, its drawn offset and the marks where they were"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn list_keeps_its_place_when_a_transfer_finishes(cx: &mut TestAppContext) {
    keeps_its_place_after_a_move_out(cx, "list");
}

#[gpui::test]
fn grid_keeps_its_place_when_a_transfer_finishes(cx: &mut TestAppContext) {
    keeps_its_place_after_a_move_out(cx, "grid");
}

#[gpui::test]
fn columns_keeps_its_place_when_a_transfer_finishes(cx: &mut TestAppContext) {
    keeps_its_place_after_a_move_out(cx, "columns");
}

// The cursor's own file left: the cursor takes the row that moved into its slot, as after a delete of that row, and the mark of the file that left goes.
#[gpui::test]
fn the_cursor_keeps_its_slot_when_its_own_file_was_moved_away(cx: &mut TestAppContext) {
    let (shell, cx, before) = deep(cx, "list");
    let at: usize = before.cursor[1..].parse().unwrap();
    finish(&shell, cx, 0, &without(&[name(at)]));
    assert_eq!(
        place(&shell, cx, 0),
        Place {
            cursor: name(at + 1),
            offset: before.offset,
            marked: vec![name(at - 3)],
        },
        "the next file took the slot the moved file left, at the same drawn offset"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// A paste into the shown folder: what arrives sorts into the listing and takes neither the cursor nor the scroll.
#[gpui::test]
fn arrivals_from_a_paste_take_neither_the_cursor_nor_the_scroll(cx: &mut TestAppContext) {
    let (shell, cx, before) = deep(cx, "list");
    let mut after = folder();
    for arrival in ["a-first", "n0500-pasted", "z-last"] {
        after.push(row(arrival, false));
    }
    after.sort_by(|a, b| a.name.cmp(&b.name));
    finish(&shell, cx, 0, &after);
    assert_eq!(
        place(&shell, cx, 0),
        before,
        "the pasted files joined the listing around a view that did not move"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// A move between two shown folders: the pane that ran it hears the transfer end, the other hears its folder change.
#[gpui::test]
fn both_shown_folders_keep_their_place_after_a_move_between_them(cx: &mut TestAppContext) {
    let (shell, cx, source) = deep(cx, "list");
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Dual, window, cx)));
    let right = asked_on(&shell, cx, 1);
    serve(&shell, cx, 1, &right, &folder());
    cx.simulate_keystrokes("tab end up up");
    serve(&shell, cx, 1, &right, &folder());
    assert_eq!(shell.read_with(cx, |shell, _| shell.focus_side), 1);
    let destination = place(&shell, cx, 1);
    assert_eq!(destination.cursor, name(TOTAL - 3));
    let source_now = place(&shell, cx, 0);
    assert_eq!(source_now.cursor, source.cursor);
    let mut arrived = folder();
    arrived.insert(0, row("a-moved", false));
    receive(
        &shell,
        cx,
        0,
        Backend::test_reply_line(r#"{"t":"changed","path":"/files"}"#),
    );
    finish(&shell, cx, 1, &arrived);
    cx.executor()
        .advance_clock(WATCH + Duration::from_millis(1));
    cx.run_until_parked();
    serve(&shell, cx, 0, "/files", &without(&[name(100)]));
    assert_eq!(
        place(&shell, cx, 1),
        destination,
        "the destination pane kept its place"
    );
    assert_eq!(
        place(&shell, cx, 0),
        source_now,
        "the source pane kept its place"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// Columns with the changed folder drawn as the parent column: the active column keeps its cursor row and scroll.
#[gpui::test]
fn the_active_column_keeps_its_place_when_its_parent_column_changed(cx: &mut TestAppContext) {
    let (shell, cx, before) = deep(cx, "columns");
    let lists_before = lists(&shell, cx, 0);
    receive(
        &shell,
        cx,
        0,
        Backend::test_reply_line(r#"{"t":"changed","path":"/"}"#),
    );
    finish(&shell, cx, 0, &folder());
    assert_eq!(
        place(&shell, cx, 0),
        before,
        "the active column did not return to its first row"
    );
    assert_eq!(
        lists(&shell, cx, 0),
        lists_before + 1,
        "the active column was read once"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// A transfer of many items: the folder's change notices and the transfer's end are one re-read, not one each.
#[gpui::test]
fn a_finished_transfer_and_its_change_notices_are_one_reread(cx: &mut TestAppContext) {
    let (shell, cx, before) = deep(cx, "list");
    let lists_before = lists(&shell, cx, 0);
    for _ in 0..3 {
        receive(
            &shell,
            cx,
            0,
            Backend::test_reply_line(r#"{"t":"changed","path":"/files"}"#),
        );
    }
    finish(&shell, cx, 0, &without(&[name(100), name(101), name(102)]));
    cx.executor().advance_clock(WATCH * 3);
    cx.run_until_parked();
    serve(
        &shell,
        cx,
        0,
        "/files",
        &without(&[name(100), name(101), name(102)]),
    );
    assert_eq!(
        lists(&shell, cx, 0),
        lists_before + 1,
        "the notices that came before the transfer's end were paid by its re-read"
    );
    assert_eq!(place(&shell, cx, 0), before);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// A cancelled transfer moved nothing: the same re-read lands on the same rows and nothing the user sees changes.
#[gpui::test]
fn a_cancelled_transfer_changes_nothing(cx: &mut TestAppContext) {
    let (shell, cx, before) = deep(cx, "list");
    let path = asked_on(&shell, cx, 0);
    receive(
        &shell,
        cx,
        0,
        BackendEvent::Operations(
            r#"{"t":"transferdone","id":7,"ok":0,"failed":0,"skipped":0,"cancelled":true}"#.into(),
        ),
    );
    serve(&shell, cx, 0, &path, &folder());
    assert_eq!(place(&shell, cx, 0), before);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
