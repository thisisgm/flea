//! Row marks track selection replacement and stay allocation-free once warm.
use super::rig::*;
use super::{Landing, Take};
use crate::list::bench::allocations;
use flea::ui_logic_dg1::PendingPaste;
use std::time::Instant;

fn take(path: &str) -> Take<'_> {
    Rig::take_facts(path)
}

fn landing(path: &str) -> Landing<'_> {
    Rig::landing(path)
}

// ClipMarks: one lookup build per clipboard, then every row is a single key read, whatever the clip size.
#[test]
fn a_row_mark_costs_one_key_read_not_a_copy_of_the_clip() {
    fn timed(count: usize) -> (u128, u64) {
        let mut rig = Rig::new();
        let clip: Vec<String> = (0..count).map(|at| format!("/dest/f{at}")).collect();
        rig.board.take(
            &rig.en,
            7,
            true,
            Some(clip),
            Vec::new(),
            take("/dest"),
            &rig.wire,
            &mut rig.sink,
            &mut rig.owner,
        );
        assert_eq!(rig.board.mark_for_row("/dest", "f0"), "scissors");
        let start = Instant::now();
        for at in 0..50 {
            rig.board
                .mark_for_row("/dest", if at % 2 == 0 { "f7" } else { "absent" });
        }
        (start.elapsed().as_micros(), rig.board.builds)
    }
    let (small, small_builds) = timed(1);
    let (large, large_builds) = timed(100_000);
    assert_eq!((small_builds, large_builds), (1, 1));
    assert!(
        large < 50_000,
        "50 row marks: 1-path clip {small} us, 100000-path clip {large} us"
    );
}

// A fallback read replaces the selection in place; the marks are keyed by the session revision, so that replacement must move it or the old lookup stays.
#[test]
fn a_fallback_read_refreshes_the_marks() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    rig.board.take(
        &rig.en,
        7,
        false,
        Some(vec!["/old/x".into()]),
        Vec::new(),
        take("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(rig.board.mark_for_row("/old", "x"), "copy");
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"set","ok":true,"token":""}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"changed","clip":"none","error":"no data-control"}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(rig.board.watch_failed(7));
    let said = rig.board.paste(
        &rig.en,
        7,
        PendingPaste::transfer("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(said.is_empty());
    let (claimed, _) = rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"get","ok":true,"clip":"copy","paths":["/fallback/file"],"token":""}"#,
        take("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert!(claimed);
    assert_eq!(
        rig.board.mark_for_row("/fallback", "file"),
        "copy",
        "the read's selection marks its row"
    );
    assert_eq!(
        rig.board.mark_for_row("/old", "x"),
        "",
        "the replaced selection marks nothing"
    );
}

// A row mark on an empty clipboard returns before any join, and a cut row is one key read, so 50 marks allocate nothing (ui/js/ClipMarks.js:45-56, rule 7).
#[test]
fn a_row_mark_allocates_nothing_per_row() {
    let mut rig = Rig::new();
    rig.board.mark_for_row("/dest", "f7");
    let before = allocations();
    for _ in 0..50 {
        rig.board.mark_for_row("/dest", "f7");
    }
    let empty = allocations() - before;
    rig.board.take(
        &rig.en,
        7,
        true,
        Some(vec!["/dest/f7".into()]),
        Vec::new(),
        take("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    assert_eq!(rig.board.mark_for_row("/dest", "f7"), "scissors");
    let before = allocations();
    for _ in 0..50 {
        rig.board.mark_for_row("/dest", "f7");
    }
    let full = allocations() - before;
    assert_eq!(
        (empty, full),
        (0, 0),
        "heap allocations over 50 row marks: empty clipboard {empty}, one-path cut {full}"
    );
}
