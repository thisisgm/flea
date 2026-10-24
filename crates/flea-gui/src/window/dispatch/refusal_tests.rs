//! A refused step says why in the pane status, and a reload past the listing wait asks again for the folder waited on.
use super::test_shell::{asked, land, land_on, lists, open, row, set_cursor, status};
use crate::{navigation::Refusal, state::Shell, window::startup::LISTING_WAIT_MS};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

fn said(shell: &Entity<Shell>, cx: &mut VisualTestContext, refusal: Refusal) -> String {
    shell.read_with(cx, |shell, _| shell.locale.catalog.tr(refusal.key()))
}

fn waiting(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().waiting)
}

fn home() -> Vec<flea::protocol::RowFacts> {
    vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ]
}

#[gpui::test]
fn a_reload_while_listing_refuses_and_past_the_wait_asks_again_for_the_folder_waited_on(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("enter");
    assert_eq!(asked(&shell, cx), "/files/beta");
    let out = lists(&shell, cx);
    // Reload.js begin: a listing out refuses itself.
    cx.simulate_keystrokes("f5");
    assert_eq!(status(&shell, cx), said(&shell, cx, Refusal::Busy));
    assert_eq!(lists(&shell, cx), out, "a busy reload lists nothing");
    cx.executor()
        .advance_clock(Duration::from_millis(LISTING_WAIT_MS + 1));
    cx.run_until_parked();
    assert!(waiting(&shell, cx), "the listing passed its wait");
    // Past the wait the folder waited on is asked again, never the folder drawn.
    cx.simulate_keystrokes("f5");
    assert_eq!(lists(&shell, cx), out + 1, "the reload retries the listing");
    assert_eq!(asked(&shell, cx), "/files/beta");
    assert!(!waiting(&shell, cx));
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 2);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_reload_of_the_shown_folder_past_the_wait_reads_it_again(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    cx.simulate_keystrokes("f5");
    let out = lists(&shell, cx);
    assert!(shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.busy()));
    cx.executor()
        .advance_clock(Duration::from_millis(LISTING_WAIT_MS + 1));
    cx.run_until_parked();
    assert!(waiting(&shell, cx));
    cx.simulate_keystrokes("f5");
    assert_eq!(lists(&shell, cx), out + 1);
    assert_eq!(asked(&shell, cx), "/files");
    land(&shell, cx, "/files", home(), 2);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn enter_on_an_unloaded_row_says_so_and_page_forward_stays_quiet(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    // Ten rows more than the first window: the cursor below it names a row the window has not read.
    let first = shell.read_with(cx, |shell, _| {
        let pending = shell.panes[0].as_ref().unwrap().pane.swap.pending.as_ref();
        pending.unwrap().first
    });
    let rows = (0..first)
        .map(|at| row(&format!("folder{at}"), true))
        .collect();
    land_on(&shell, cx, 0, "/files", rows, first + 10, 1);
    set_cursor(&shell, cx, first + 5);
    let before = (lists(&shell, cx), status(&shell, cx));
    // Focus.js:133-139: pageForward with no row under the cursor is quiet.
    cx.simulate_keystrokes("right");
    assert_eq!((lists(&shell, cx), status(&shell, cx)), before);
    // Nav.js:213-226 openableRow: Enter there says the row has not loaded.
    cx.simulate_keystrokes("enter");
    assert_eq!(status(&shell, cx), said(&shell, cx, Refusal::NotLoaded));
    assert_eq!(lists(&shell, cx), before.0);
    assert_eq!(asked(&shell, cx), "/files");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
