// Ported from the pinned half of tests/js/settingsshelf.js; the rows it draws are the settings unit's.
use super::shelfpile::*;

#[test]
fn only_pinned_entries_with_a_path_are_pins() {
    let pile = r#"{"items":[{"path":"/home/gm/Invoices","folder":true,"pinned":true},{"path":"/home/gm/loose.txt","folder":false,"pinned":false},{"path":"/home/gm/Work/","folder":true,"pinned":true,"missing":true},{"folder":true,"pinned":true},{"path":"/home/gm/nope","pinned":"yes"}]}"#;
    let pins = pinned(pile);
    assert_eq!(
        pins.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["Invoices", "Work"],
        "a trailing slash is not part of the name"
    );
    assert_eq!(
        (pins[0].folder, pins[0].missing, pins[1].missing),
        (true, false, true),
        "a pin says folder and gone"
    );
    assert_eq!(
        pins[1].path, "/home/gm/Work/",
        "the path is kept as written"
    );
    assert!(
        pinned(r#"{"items":[{"#).is_empty() && pinned("").is_empty(),
        "an unparsable pile is no pins"
    );
    assert!(
        pinned(r#"{"items":{"path":"/a"}}"#).is_empty(),
        "items that are not a list are no pins"
    );
}

#[test]
fn the_pile_lives_in_the_state_home() {
    assert_eq!(
        pile_file("/run/state", "/home/gm"),
        "/run/state/omarchy/flea-shelf/shelf.json"
    );
    assert_eq!(
        pile_file("", "/home/gm"),
        "/home/gm/.local/state/omarchy/flea-shelf/shelf.json"
    );
}

fn pins(paths: &[&str]) -> Pins {
    let records = paths
        .iter()
        .map(|p| Pin {
            path: (*p).into(),
            name: (*p).into(),
            folder: true,
            missing: false,
        })
        .collect();
    Pins {
        records,
        ..Pins::default()
    }
}

#[test]
fn calls_run_one_at_a_time_in_the_order_asked() {
    let mut queue = pins(&["/a", "/b"]);
    assert_eq!(
        queue.run(Call::Pin("/c".into())),
        Some(Call::Pin("/c".into())),
        "an idle queue starts the call"
    );
    assert_eq!(
        queue.run(Call::Unpin("/c".into())),
        None,
        "a call behind it waits"
    );
    assert_eq!(queue.step("/a", 1), None, "and so does a move");
    assert_eq!(
        queue.exited(0),
        Some(Call::Unpin("/c".into())),
        "the pin lands before its unpin starts"
    );
    assert_eq!(queue.last_error, None);
    assert_eq!(
        queue.exited(3),
        Some(Call::Order("/a".into(), 1)),
        "then the move"
    );
    assert_eq!(
        queue.last_error,
        Some("places.shelf.unpin_failed"),
        "a refused unpin says so"
    );
    assert_eq!(queue.exited(1), None, "nothing is left");
    assert_eq!(queue.last_error, Some("places.shelf.pin_failed"));
    assert_eq!(
        Call::Order("/a".into(), 1).args(),
        ["shelf", "order", "/a", "1"]
    );
    assert_eq!(Call::Pin("/a".into()).args(), ["shelf", "pin", "/a"]);
    assert_eq!(Call::Unpin("/a".into()).args(), ["shelf", "unpin", "/a"]);
}

#[test]
fn a_move_off_either_end_does_nothing() {
    let mut list = pins(&["/a", "/b"]);
    assert_eq!(list.step("/a", -1), None, "the first pin cannot move up");
    assert_eq!(list.step("/b", 1), None, "the last pin cannot move down");
    assert_eq!(
        list.step("/gone", 1),
        None,
        "a pin no longer listed does nothing"
    );
    assert_eq!(list.move_to("/b", 0), Some(Call::Order("/b".into(), 0)));
}
