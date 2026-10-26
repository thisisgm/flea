//! A cursor key pressed while the open folder is read again waits for the re-read and lands on it, on a 100k listing.
use super::{
    rename_tests::{close, draw, receive, sent, Lines},
    test_shell::{land_on, lists, open, row, set_cursor},
};
use crate::{
    backend::{Backend, Event as BackendEvent, Failure},
    state::Shell,
    window::navigate::Step,
};
use gpui::{Entity, TestAppContext, VisualTestContext};

pub(super) const TOTAL: usize = 100_000;

fn names(n: usize) -> Vec<String> {
    (0..n).map(|at| format!("note{at:03}.txt")).collect()
}

fn rows(names: &[String]) -> Vec<flea::protocol::RowFacts> {
    names.iter().map(|name| row(name, false)).collect()
}

// /files lands with 100k rows, its first window held, the cursor on row 1, and the backend's request lines read back.
pub(super) fn hundred_k(
    cx: &mut TestAppContext,
) -> (Entity<Shell>, &mut VisualTestContext, Lines, Vec<String>) {
    let (shell, cx) = open(cx, "default");
    let first = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .swap
            .pending
            .as_ref()
            .unwrap()
            .first
    });
    let all = names(first);
    land_on(&shell, cx, 0, "/files", rows(&all), TOTAL, 1);
    set_cursor(&shell, cx, 1);
    let (backend, lines) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    (shell, cx, Box::new(lines), all)
}

// The re-read's answer: its own first window of the same 100k folder.
pub(super) fn land_reread(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    path: &str,
    names: &[String],
    total: usize,
    listing: usize,
) {
    let first = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .swap
            .pending
            .as_ref()
            .unwrap()
            .first
    });
    let count = first.min(total);
    let window: Vec<String> = (0..count)
        .map(|at| {
            names
                .get(at)
                .cloned()
                .unwrap_or_else(|| format!("note{at:03}.txt"))
        })
        .collect();
    land_on(shell, cx, 0, path, rows(&window), total, listing);
    assert!(!busy(shell, cx), "the re-read landed");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .held
            .total),
        total
    );
}

pub(super) fn cursor(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> usize {
    shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor)
}

fn busy(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.busy())
}

pub(super) fn key(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    draw(cx);
}

// F5 starts the same-folder re-read the rename reply also starts.
fn reread(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let before = lists(shell, cx);
    key(cx, "f5");
    assert_eq!(lists(shell, cx), before + 1, "F5 reads the folder again");
    assert!(busy(shell, cx), "the re-read is out");
}

#[gpui::test]
fn gh265_down_during_a_plain_f5_reread_lands_once_the_reread_does(cx: &mut TestAppContext) {
    let (shell, cx, _lines, all) = hundred_k(cx);
    reread(&shell, cx);
    key(cx, "down");
    assert_eq!(
        cursor(&shell, cx),
        1,
        "the held rows do not move under the re-read"
    );
    land_reread(&shell, cx, "/files", &all, TOTAL, 2);
    assert_eq!(
        cursor(&shell, cx),
        2,
        "the Down pressed during the re-read moves the cursor once it lands"
    );
    close(&shell, cx);
}

#[gpui::test]
fn gh265_down_after_a_rename_reply_lands_on_the_reread(cx: &mut TestAppContext) {
    let (shell, cx, mut lines, mut all) = hundred_k(cx);
    set_cursor(&shell, cx, 2);
    key(cx, "f2");
    cx.simulate_input("draft");
    key(cx, "enter");
    assert_eq!(sent(&mut lines).len(), 1, "the rename is out");
    let before = lists(&shell, cx);
    receive(
        &shell,
        cx,
        BackendEvent::Producer(r#"{"t":"renamed","ok":true,"path":"/files/draft.txt"}"#.into()),
    );
    assert_eq!(
        lists(&shell, cx),
        before + 1,
        "the renamed reply reads the folder again"
    );
    assert!(busy(&shell, cx), "the re-read is out");
    key(cx, "down");
    all[2] = "draft.txt".into();
    land_reread(&shell, cx, "/files", &all, TOTAL, 2);
    assert_eq!(
        cursor(&shell, cx),
        3,
        "Down after the rename lands one row past the renamed file"
    );
    close(&shell, cx);
}

#[gpui::test]
fn gh265_the_reread_guard_still_refuses_work_and_keeps_every_cursor_key(cx: &mut TestAppContext) {
    let (shell, cx, _lines, all) = hundred_k(cx);
    set_cursor(&shell, cx, 5);
    reread(&shell, cx);
    // A mark must wait for the rows the re-read renumbers, so it is refused, never held for later.
    key(cx, "v");
    for keys in ["down", "down", "down", "up"] {
        key(cx, keys);
    }
    assert_eq!(
        cursor(&shell, cx),
        5,
        "nothing moves while the re-read is out"
    );
    let marked = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            shell.panes[0].as_ref().unwrap().selected.clone()
        })
    };
    assert!(marked(&shell, cx).is_empty(), "the guard refuses the mark");
    land_reread(&shell, cx, "/files", &all, TOTAL, 2);
    assert_eq!(
        cursor(&shell, cx),
        7,
        "three Downs and an Up all count once the re-read lands"
    );
    assert!(
        marked(&shell, cx).is_empty(),
        "the refused mark is not replayed"
    );
    // The debt is paid once: a later commit of the same folder does not move the cursor again.
    reread(&shell, cx);
    land_reread(&shell, cx, "/files", &all, TOTAL, 3);
    assert_eq!(cursor(&shell, cx), 7);
    close(&shell, cx);
}

// Held keys replay in order: a page after two Downs, Down after an Up refused at the top, an Up after End.
#[gpui::test]
fn gh265_mixed_keys_replay_in_their_order_across_the_edges(cx: &mut TestAppContext) {
    let (shell, cx, _lines, all) = hundred_k(cx);
    let page = shell.read_with(cx, |shell, _| {
        let frame = shell.panes[0].as_ref().unwrap().input.frame.unwrap();
        ((frame.viewport() / frame.row).ceil() as usize / 2).max(1)
    });
    reread(&shell, cx);
    for keys in ["down", "down", "pagedown"] {
        key(cx, keys);
    }
    land_reread(&shell, cx, "/files", &all, TOTAL, 2);
    assert_eq!(
        cursor(&shell, cx),
        1 + 2 + page,
        "two Downs and a PageDown all land, in order"
    );
    set_cursor(&shell, cx, 0);
    reread(&shell, cx);
    key(cx, "up");
    key(cx, "down");
    land_reread(&shell, cx, "/files", &all, TOTAL, 3);
    assert_eq!(
        cursor(&shell, cx),
        1,
        "Up at the top moves nothing, then Down moves one"
    );
    reread(&shell, cx);
    key(cx, "end");
    key(cx, "up");
    land_reread(&shell, cx, "/files", &all, TOTAL, 4);
    assert_eq!(
        cursor(&shell, cx),
        TOTAL - 2,
        "End then Up ends one row above the last"
    );
    close(&shell, cx);
}

// A mark the re-read's window lacks is located first; the held key waits for that answer, then lands.
#[gpui::test]
fn gh265_a_held_key_waits_for_the_marks_the_reread_locates(cx: &mut TestAppContext) {
    let (shell, cx, _lines, mut all) = hundred_k(cx);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![300]
    });
    reread(&shell, cx);
    key(cx, "down");
    all[300] = "moved300.txt".into();
    land_reread(&shell, cx, "/files", &all, TOTAL, 2);
    let id = shell.read_with(cx, |shell, _| {
        let anchor = shell.panes[0].as_ref().unwrap().nav.anchor.as_ref();
        anchor.and_then(|anchor| anchor.locate)
    });
    let id = id.expect("the mark outside the window is located");
    assert_eq!(
        cursor(&shell, cx),
        1,
        "the key waits while the marks are owed"
    );
    let located = crate::backend::Located {
        directory: "/files".into(),
        path: String::new(),
        index: -1,
        id: Some(id),
        transfer: 0,
        ok: true,
        matches: vec![("/files/note300.txt".into(), 5000)],
    };
    receive(&shell, cx, BackendEvent::Located(located));
    assert_eq!(
        cursor(&shell, cx),
        2,
        "the held Down lands with the located marks"
    );
    close(&shell, cx);
}

#[gpui::test]
fn gh265_a_held_key_is_dropped_when_navigation_leaves_the_folder(cx: &mut TestAppContext) {
    let (shell, cx, _lines, _all) = hundred_k(cx);
    reread(&shell, cx);
    key(cx, "down");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let _ = shell.step(0, Step::Open("/other".into()), window, cx);
        })
    });
    cx.run_until_parked();
    let other = names(40);
    land_reread(&shell, cx, "/other", &other, other.len(), 3);
    assert_eq!(
        cursor(&shell, cx),
        0,
        "the key held for /files never moves the /other cursor"
    );
    close(&shell, cx);
}

#[gpui::test]
fn gh265_a_key_pressed_while_another_folder_loads_is_not_replayed(cx: &mut TestAppContext) {
    let (shell, cx, _lines, _all) = hundred_k(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let _ = shell.step(0, Step::Open("/other".into()), window, cx);
        })
    });
    cx.run_until_parked();
    assert!(busy(&shell, cx), "the /other load is out");
    for _ in 0..3 {
        key(cx, "down");
    }
    let other = names(40);
    land_reread(&shell, cx, "/other", &other, other.len(), 2);
    assert_eq!(
        cursor(&shell, cx),
        0,
        "keys pressed during a load into another folder are swallowed as before"
    );
    close(&shell, cx);
}

#[gpui::test]
fn gh265_a_failed_reread_drops_its_held_key(cx: &mut TestAppContext) {
    let (shell, cx, _lines, all) = hundred_k(cx);
    reread(&shell, cx);
    key(cx, "down");
    receive(
        &shell,
        cx,
        BackendEvent::Failed(Failure {
            key: "window.backend_eof",
            detail: String::new(),
        }),
    );
    assert!(!busy(&shell, cx), "the failed re-read is out");
    // A failed backend read clears the rows, so the cursor the failure leaves is the baseline.
    let after = cursor(&shell, cx);
    reread(&shell, cx);
    land_reread(&shell, cx, "/files", &all, TOTAL, 3);
    assert_eq!(
        cursor(&shell, cx),
        after,
        "a failed re-read leaves no held key behind"
    );
    close(&shell, cx);
}
