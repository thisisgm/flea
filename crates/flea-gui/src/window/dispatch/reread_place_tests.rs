//! What each re-read that does not navigate does to the user's place, deep in a long folder after a file above the cursor left.
use super::{deep, finish, lists, name, place, receive, serve, without, Place, DONE, WATCH};
use crate::{backend::Event as BackendEvent, state::Shell, window::navigate::Step};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn after(cx: &mut TestAppContext, step: Step) -> (Place, Place) {
    let (shell, cx, before) = deep(cx, "list");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let _ = shell.step(0, step, window, cx);
        })
    });
    cx.run_until_parked();
    serve(&shell, cx, 0, "/files", &without(&[name(100)]));
    let after = place(&shell, cx, 0);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    (before, after)
}

fn index(place: &Place) -> usize {
    place.cursor[1..].parse().unwrap()
}

// The watched re-read, a permissions change and a rename under a moved cursor: the place holds by file.
#[gpui::test]
fn the_anchored_reread_keeps_the_place_by_file(cx: &mut TestAppContext) {
    let (before, after) = after(cx, Step::Reread);
    assert_eq!(after, before);
}

// Reload, Properties and Make Executable.
#[gpui::test]
fn a_reload_keeps_the_place_by_file(cx: &mut TestAppContext) {
    let (before, after) = after(cx, Step::Reload);
    assert_eq!(after, before);
}

// Undo, redo, a new link and a bulk rename refresh with no file named: the view stays, the cursor holds its row number, so it stands on the next file, and the marks go.
#[gpui::test]
fn a_refresh_without_a_target_keeps_the_row_number_and_drops_the_marks(cx: &mut TestAppContext) {
    let (before, after) = after(cx, Step::Refresh(None));
    assert_eq!(
        after,
        Place {
            cursor: name(index(&before) + 1),
            offset: before.offset,
            marked: Vec::new(),
        }
    );
}

// A finished extract or compress takes the first row and clears the marks (EY-255).
#[gpui::test]
fn an_archive_result_returns_to_the_first_row(cx: &mut TestAppContext) {
    let (_, after) = after(cx, Step::Fresh);
    assert_eq!(
        after,
        Place {
            cursor: name(0),
            offset: 0.,
            marked: Vec::new(),
        }
    );
}

// List requests, window requests, rows held and the first window's size.
fn cost(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (usize, usize, usize, usize) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (
            host.nav.counters.lists,
            host.nav.counters.windows + host.pane.requests.sent,
            host.pane.held.rows.len(),
            host.pane.first_count(),
        )
    })
}

// The re-read a finished transfer starts costs what the reopen it replaced cost: one list request, no window request, one first window of rows.
#[gpui::test]
fn a_finished_transfer_reads_one_first_window(cx: &mut TestAppContext) {
    for view in ["list", "grid", "columns"] {
        let (shell, cx, _) = deep(cx, view);
        let (lists, windows, _, first) = cost(&shell, cx);
        finish(&shell, cx, 0, &without(&[name(100)]));
        assert_eq!(
            cost(&shell, cx),
            (lists + 1, windows, first, first),
            "{view}"
        );
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

// A transfer that ends while the previous one's re-read is out, with nothing marked to locate: the held debt is read once that listing lands.
#[gpui::test]
fn a_transfer_ending_during_a_reread_is_read_after_it(cx: &mut TestAppContext) {
    let (shell, cx, _) = deep(cx, "list");
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.selected.clear();
            host.input.selection.sync(&host.selected);
        })
    });
    let lists_before = lists(&shell, cx, 0);
    receive(&shell, cx, 0, BackendEvent::Operations(DONE.into()));
    let busy = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        host.nav.busy(&host.pane)
    });
    assert!(busy, "the first re-read is out");
    receive(&shell, cx, 0, BackendEvent::Operations(DONE.into()));
    assert_eq!(
        lists(&shell, cx, 0),
        lists_before + 1,
        "the second transfer's re-read is held while the first is out"
    );
    serve(&shell, cx, 0, "/files", &without(&[name(100)]));
    cx.executor().advance_clock(WATCH * 3);
    cx.run_until_parked();
    serve(&shell, cx, 0, "/files", &without(&[name(100), name(101)]));
    assert_eq!(
        lists(&shell, cx, 0),
        lists_before + 2,
        "the second transfer's folder was read after the first re-read landed"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[path = "sizeblink_reread_tests.rs"]
mod sizeblink;
