//! Entry retention follows current selections, pending set replies and live paste captures.
use super::Session;
use crate::ui_logic_dg1::harness::english;

#[test]
fn r4_copy_cycles_without_captures_do_not_keep_answered_history() {
    let en = english();
    let mut session = Session::new();
    let mut entries = Vec::new();
    for cycle in 0..1024 {
        session.set(7, vec!["/src/B".into()], false);
        entries.push(session.current_entry.weak());
        session.on_set(&en, 7, true, &format!("token{cycle}"), "");
        assert_eq!(entries.iter().filter(|entry| entry.strong_count() > 0).count(), 1);
    }
    session.foreign("none", Vec::new(), String::new());
    assert_eq!(session.pending_sets(), 0);
    assert!(!session.available());
    assert_eq!(entries.iter().filter(|entry| entry.strong_count() > 0).count(), 0,
        "no captures or pending replies can refer to any answered entry");

    session.set(7, vec!["/src/B".into()], true);
    let clip = session.current().clone();
    let first = session.capture();
    let second = session.capture();
    let held = first.weak();
    session.on_set(&en, 7, true, "captured", "");
    session.foreign("none", Vec::new(), String::new());
    assert_eq!(held.strong_count(), 2);
    let out = session.spent(&clip, &first, true);
    assert_eq!(out, vec![super::Out::Send(r#"{"c":"clipClear","token":"captured"}"#.into())]);
    drop(first);
    assert_eq!(held.strong_count(), 1, "the other capture still needs the answered token");
    assert_eq!(second.token(), "captured");
    drop(second);
    assert_eq!(held.strong_count(), 0, "cancelling the last capture releases the entry");

    for answer_first in [false, true] {
        session.set(7, vec!["/src/B".into()], false);
        let cancelled = session.capture();
        let held = cancelled.weak();
        session.foreign("none", Vec::new(), String::new());
        if answer_first {
            session.on_set(&en, 7, true, "cancelled", "");
        }
        drop(cancelled);
        assert_eq!(held.strong_count(), usize::from(!answer_first));
        if !answer_first {
            session.on_set(&en, 7, true, "cancelled", "");
        }
        assert_eq!(held.strong_count(), 0, "a late answer retains no abandoned copy history");
    }
}
