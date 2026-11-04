use super::rig::*;
use flea::ui_logic_dg1::PendingPaste;

fn cut(rig: &mut Rig) {
    rig.board.take(
        &rig.en,
        7,
        true,
        Some(vec!["/src/B".into()]),
        Vec::new(),
        Rig::take_facts("/src"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
}

fn paste(rig: &mut Rig) {
    rig.board.paste(
        &rig.en,
        7,
        PendingPaste::transfer("/dest"),
        Rig::landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
}

fn line(rig: &mut Rig, text: &str) {
    rig.board.line(
        &rig.en,
        7,
        text,
        Rig::take_facts("/src"),
        Rig::landing("/dest"),
        &rig.wire,
        &mut rig.sink,
        &mut rig.owner,
    );
}

#[test]
fn r4_two_captures_keep_the_token_until_both_complete() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    cut(&mut rig);
    paste(&mut rig);
    paste(&mut rig);
    let asks = rig.sink.asks.borrow().clone();
    assert_eq!(asks.len(), 2);
    let first = &asks[0].1;
    let second = &asks[1].1;
    assert!(first.token.is_empty() && second.token.is_empty());
    line(
        &mut rig,
        r#"{"t":"clip","op":"set","ok":true,"token":"token1"}"#,
    );
    rig.board
        .spent(7, first, true, &rig.wire, &mut rig.sink, &mut rig.owner);
    cut(&mut rig);
    line(
        &mut rig,
        r#"{"t":"clip","op":"set","ok":true,"token":"token2"}"#,
    );
    rig.board
        .spent(7, second, true, &rig.wire, &mut rig.sink, &mut rig.owner);
    let clear = rig.wire.sent.borrow().last().cloned().unwrap();
    assert_eq!(
        (clear.as_str(), rig.board.available(), rig.board.token()),
        (r#"{"c":"clipClear","token":"token1"}"#, true, "token2"),
    );
}

#[test]
fn r4_a_fallback_read_replacement_survives_an_older_spent() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    cut(&mut rig);
    line(
        &mut rig,
        r#"{"t":"clip","op":"set","ok":true,"token":"token1"}"#,
    );
    line(
        &mut rig,
        r#"{"t":"clip","op":"changed","clip":"cut","paths":["/src/B"],"token":"token1"}"#,
    );
    paste(&mut rig);
    let captured = rig.sink.asks.borrow()[0].1.clone();
    line(
        &mut rig,
        r#"{"t":"clip","op":"changed","error":"watch down"}"#,
    );
    paste(&mut rig);
    assert_eq!(rig.wire.sent.borrow().last().unwrap(), r#"{"c":"clipGet"}"#);
    line(
        &mut rig,
        r#"{"t":"clip","op":"get","ok":true,"clip":"copy","paths":["/src/B"],"token":"foreign2"}"#,
    );
    assert_eq!(rig.board.token(), "foreign2");
    assert_eq!(rig.board.mark_for_row("/src", "B"), "copy");
    rig.board
        .spent(7, &captured, true, &rig.wire, &mut rig.sink, &mut rig.owner);
    let clear = rig.wire.sent.borrow().last().cloned().unwrap();
    let mark = rig.board.mark_for_row("/src", "B");
    assert_eq!(
        (
            clear.as_str(),
            rig.board.available(),
            rig.board.token(),
            mark
        ),
        (
            r#"{"c":"clipClear","token":"token1"}"#,
            true,
            "foreign2",
            "copy"
        ),
    );
}

#[test]
fn r4_a_foreign_cut_from_a_read_clears_by_its_own_token() {
    let mut rig = Rig::new();
    rig.board.watch(7, &rig.wire);
    cut(&mut rig);
    line(
        &mut rig,
        r#"{"t":"clip","op":"set","ok":true,"token":"token1"}"#,
    );
    line(
        &mut rig,
        r#"{"t":"clip","op":"changed","error":"watch down"}"#,
    );
    paste(&mut rig);
    line(
        &mut rig,
        r#"{"t":"clip","op":"get","ok":true,"clip":"cut","paths":["/foreign/C"],"token":"foreign2"}"#,
    );
    let captured = rig.sink.asks.borrow()[0].1.clone();
    assert_eq!(captured.token, "foreign2");
    rig.board
        .spent(7, &captured, true, &rig.wire, &mut rig.sink, &mut rig.owner);
    let clear = rig.wire.sent.borrow().last().cloned().unwrap();
    assert_eq!(clear, r#"{"c":"clipClear","token":"foreign2"}"#);
}
