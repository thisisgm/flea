use super::{chord_label, KeycapSnapshot};

#[test]
fn keymap_plus_board_modified_names_and_bare_symbols() {
    let snapshot = KeycapSnapshot::new("us", None, None, 1).unwrap();
    for chord in [
        "ctrl-shift-plus",
        "ctrl-shift-minus",
        "ctrl-plus",
        "ctrl-minus",
        "ctrl-equal",
        "ctrl-comma",
        "ctrl-shift-equal",
        "ctrl-shift-greater",
        "ctrl-shift-underscore",
        "super-bracketleft",
        "super-bracketright",
        "super-shift-greater",
        "+",
        "-",
    ] {
        assert_eq!(chord_label(chord, &snapshot).unwrap().text, chord);
    }
    assert_eq!(
        snapshot.label("supershift-period"),
        Ok("super-shift-period")
    );
    assert_eq!(
        snapshot.label("supershift-greater"),
        Ok("super-shift-greater")
    );
    assert_eq!(snapshot.label("alt-return"), Ok("alt-enter"));
    assert_eq!(snapshot.label("shift-backtab"), Ok("shift-tab"));
}

#[test]
fn keymap_plus_layout_override_and_sequence_preserved() {
    let snapshot = KeycapSnapshot::new("de", None, None, 2).unwrap();
    assert_eq!(snapshot.label("ctrl-minus"), Ok("ctrl-ß"));
    assert_eq!(
        chord_label("ctrl-shift-plus / ctrl-shift-minus", &snapshot)
            .unwrap()
            .text,
        "ctrl-shift-plus / ctrl-shift-minus"
    );
}
