//! Clipboard capture races after a transfer is spent.
use super::*;

#[test]
fn a_cut_pasted_before_its_set_answers_is_spent_whole() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    rig.board.take(
        &rig.en,
        7,
        true,
        Some(vec!["/src/B".into()]),
        Vec::new(),
        take("/src"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.paste(
        &rig.en,
        7,
        PendingPaste::transfer("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    let (_, captured) = rig.sink.asks.borrow().last().cloned().unwrap();
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"set","ok":true,"token":"tokenB"}"#,
        take("/src"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board
        .spent(7, &captured, true, &rig.wire, &mut rig.sink, &mut rig.owner);
    let last = rig.wire.sent.borrow().last().cloned().unwrap();
    let mark = rig.board.mark_for_row("/src", "B");
    assert_eq!(
        (last.as_str(), rig.board.available(), mark),
        (r#"{"c":"clipClear","token":"tokenB"}"#, false, ""),
        "clear line, window no longer holds the cut, no mark on the moved row"
    );
}

// Cut B, paste before its set answers, take B again (cut or copy), the first set answers, then the first transfer is spent: the newer selection of the same paths survives.
fn newer_selection_after_capture(moving_again: bool) -> (String, bool, String, &'static str) {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    rig.board.take(
        &rig.en,
        7,
        true,
        Some(vec!["/src/B".into()]),
        Vec::new(),
        take("/src"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.paste(
        &rig.en,
        7,
        PendingPaste::transfer("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    let (_, captured) = rig.sink.asks.borrow().last().cloned().unwrap();
    rig.board.take(
        &rig.en,
        7,
        moving_again,
        Some(vec!["/src/B".into()]),
        Vec::new(),
        take("/src"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"set","ok":true,"token":"token1"}"#,
        take("/src"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board
        .spent(7, &captured, true, &rig.wire, &mut rig.sink, &mut rig.owner);
    let clear = rig.wire.sent.borrow().last().cloned().unwrap();
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"set","ok":true,"token":"token2"}"#,
        take("/src"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    let mark = rig.board.mark_for_row("/src", "B");
    (
        clear,
        rig.board.available(),
        rig.board.token().to_string(),
        mark,
    )
}

#[test]
fn r3_a_newer_cut_of_the_same_paths_survives_an_older_spent() {
    let got = newer_selection_after_capture(true);
    assert_eq!(
        (got.0.as_str(), got.1, got.2.as_str(), got.3),
        (
            r#"{"c":"clipClear","token":"token1"}"#,
            true,
            "token2",
            "scissors"
        ),
        "clear line, newer cut still held, its token, its mark"
    );
}

#[test]
fn r3_a_newer_copy_of_the_same_paths_survives_an_older_spent() {
    let got = newer_selection_after_capture(false);
    assert_eq!(
        (got.0.as_str(), got.1, got.2.as_str(), got.3),
        (
            r#"{"c":"clipClear","token":"token1"}"#,
            true,
            "token2",
            "copy"
        ),
        "clear line, newer copy still held, its token, its mark"
    );
}

// Control: with no newer selection the FIX2 path clears by the stamped token, as the unit's own test pins.
#[test]
fn r3_control_the_same_entry_still_clears_by_its_stamped_token() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    rig.board.take(
        &rig.en,
        7,
        true,
        Some(vec!["/src/B".into()]),
        Vec::new(),
        take("/src"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.paste(
        &rig.en,
        7,
        PendingPaste::transfer("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    let (_, captured) = rig.sink.asks.borrow().last().cloned().unwrap();
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"set","ok":true,"token":"token1"}"#,
        take("/src"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board
        .spent(7, &captured, true, &rig.wire, &mut rig.sink, &mut rig.owner);
    let clear = rig.wire.sent.borrow().last().cloned().unwrap();
    assert_eq!(
        (clear.as_str(), rig.board.available()),
        (r#"{"c":"clipClear","token":"token1"}"#, false)
    );
}

// Cut B, paste before its set answers, set answers, cut B again and its set answers, then spend: the capture clears by its own token, so the newer cut of the same paths is never nulled.
#[test]
fn r3_an_answered_capture_spent_after_a_newer_cut_clears_by_its_own_token() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    rig.board.take(
        &rig.en,
        7,
        true,
        Some(vec!["/src/B".into()]),
        Vec::new(),
        take("/src"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.paste(
        &rig.en,
        7,
        PendingPaste::transfer("/dest"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    let (_, captured) = rig.sink.asks.borrow().last().cloned().unwrap();
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"set","ok":true,"token":"token1"}"#,
        take("/src"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.take(
        &rig.en,
        7,
        true,
        Some(vec!["/src/B".into()]),
        Vec::new(),
        take("/src"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board.line(
        &rig.en,
        7,
        r#"{"t":"clip","op":"set","ok":true,"token":"token2"}"#,
        take("/src"),
        landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
    rig.board
        .spent(7, &captured, true, &rig.wire, &mut rig.sink, &mut rig.owner);
    let clear = rig.wire.sent.borrow().last().cloned().unwrap();
    assert_eq!(
        (clear.as_str(), rig.board.available(), rig.board.token()),
        (r#"{"c":"clipClear","token":"token1"}"#, true, "token2"),
        "clear line, newer cut still held, its token"
    );
}
