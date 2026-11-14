// Deferred requests, watch bursts and unmounts retain their original ordering.
use super::*;

// Requests that land while a scan is out wait for it and replay in arrival order, so list/list/paths sees both listings.
#[test]
fn requests_that_arrive_while_a_scan_is_out_replay_in_order() {
    let mut gate = Gate::new(crate::backend::events::channel().0);
    assert!(!gate.holds(), "nothing is out before the first list");
    gate.begin(Job {
        path: "/a".into(),
        first: 0,
        line: "{}".into(),
        hidden: false,
        want_changed: false,
        session: None,
    });
    assert!(gate.holds());
    gate.defer("first".into());
    gate.defer("second".into());
    assert_eq!(gate.take().map(|job| job.path), Some("/a".to_string()));
    assert_eq!(
        gate.next().as_deref(),
        Some("first"),
        "the first request replays first"
    );
    assert_eq!(
        gate.next().as_deref(),
        Some("second"),
        "and the second after it"
    );
    assert_eq!(gate.next(), None);
}

// A change seen while a scan is out is answered after it, against the descriptor that became current.
#[test]
fn a_change_seen_during_a_scan_is_decided_once_the_scan_lands() {
    let mut gate = Gate::new(crate::backend::events::channel().0);
    gate.begin(Job {
        path: "/a".into(),
        first: 0,
        line: "{}".into(),
        hidden: false,
        want_changed: false,
        session: None,
    });
    gate.note_change(7);
    gate.note_change(9);
    assert_eq!(
        gate.take_changes(),
        vec![7, 9],
        "every descriptor is kept for the decision"
    );
    assert!(
        gate.take_changes().is_empty(),
        "and the next scan starts with none"
    );
}

// An unmount seen while a scan is out is kept apart from the changes and replays in the order it arrived.
#[test]
fn an_unmount_seen_during_a_scan_waits_apart_from_the_changes_and_keeps_its_order() {
    let mut gate = Gate::new(crate::backend::events::channel().0);
    gate.begin(Job {
        path: "/a".into(),
        first: 0,
        line: "{}".into(),
        hidden: false,
        want_changed: false,
        session: None,
    });
    gate.note_change(7);
    gate.note_unmount(9);
    gate.note_unmount(4);
    assert_eq!(gate.take_changes(), vec![7], "a change is not an unmount");
    assert_eq!(
        gate.take_unmounts(),
        vec![9, 4],
        "every unmount is kept, in arrival order"
    );
    assert!(
        gate.take_unmounts().is_empty(),
        "and the next scan starts with none"
    );
}
