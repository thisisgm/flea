use super::dispatch::{event, resolve};
use super::*;
use flea::keys::{sequence::Seat, ALT, CONTROL, KEYPAD, SHIFT, SUPER};
use flea_gui::window::input::{self, Dispatch, Keyboard};
use gpui::Modifiers;

fn held(mask: u8) -> Modifiers {
    event("", None, mask).keystroke.modifiers
}

fn dispatch(expected: Match) -> Dispatch {
    match expected {
        Match::Unbound => Dispatch::Unbound,
        Match::Suppressed => Dispatch::Suppressed,
        Match::Action(action) => Dispatch::Action(action),
    }
}

#[test]
fn fork_shaped_shifted_symbol_chords_match_the_oracle_on_every_preset() {
    // keystroke_from_xkb sends the shifted keysym as the key and clears Shift for a case-less character.
    let chords = [
        (">", Some(">"), CONTROL, "Greater"),
        ("+", Some("+"), CONTROL, "Plus"),
        ("_", None, CONTROL, "Underscore"),
        (">", Some(">"), SUPER, "Greater"),
    ];
    for preset in keys::PRESETS {
        for (key, text, mask, qt_key) in chords {
            let fork = event(key, text, mask);
            let qt = |modifiers| {
                keys::lookup(
                    preset,
                    Context::Listing,
                    Frontend::Gui,
                    Input {
                        key: qt_key,
                        text: text.unwrap_or(""),
                        modifiers,
                    },
                )
            };
            assert_eq!(
                Keyboard::default().resolve(
                    preset,
                    &fork,
                    held(mask | SHIFT),
                    false,
                    Seat::default()
                ),
                dispatch(qt(mask | SHIFT)),
                "{preset} {key} {mask} with Shift held"
            );
            assert_eq!(
                Keyboard::default().resolve(preset, &fork, held(mask), false, Seat::default()),
                dispatch(qt(mask)),
                "{preset} {key} {mask} on a layout with the symbol unshifted"
            );
        }
        let control = matches!(preset, "default" | "vim" | "windows");
        for (key, text, mask, expected) in [
            (">", Some(">"), CONTROL, control.then_some("toggleHidden")),
            ("+", Some("+"), CONTROL, control.then_some("textSizeUp")),
            ("_", None, CONTROL, control.then_some("textSizeDown")),
            (
                ">",
                Some(">"),
                SUPER,
                (preset == "mac").then_some("toggleHidden"),
            ),
        ] {
            let Some(expected) = expected else { continue };
            assert_eq!(
                Keyboard::default().resolve(
                    preset,
                    &event(key, text, mask),
                    held(mask | SHIFT),
                    false,
                    Seat::default()
                ),
                Dispatch::Action(expected),
                "{preset} {key} {mask}"
            );
        }
    }
    let mut keyboard = Keyboard::default();
    for (key, text, mask, expected) in [
        ("tab", None, SHIFT, Dispatch::Action("focusPrevious")),
        ("n", None, CONTROL | SHIFT, Dispatch::Action("newFolder")),
        ("g", Some("G"), SHIFT, Dispatch::Action("cursorLast")),
        (
            "pageup",
            None,
            CONTROL | SHIFT,
            Dispatch::Action("tabMoveLeft"),
        ),
    ] {
        assert_eq!(
            keyboard.resolve(
                "default",
                &event(key, text, mask),
                held(mask),
                false,
                Seat::default()
            ),
            expected,
            "{key} {mask}"
        );
    }
}

#[test]
fn escape_reads_a_live_pair_before_the_sequence_disarms_it() {
    let mut keyboard = Keyboard::default();
    let seat = Seat::default();
    let escape = event("escape", None, 0);
    let y = event("y", Some("y"), 0);
    assert!(!keyboard.pair_armed("vim", seat));
    assert_eq!(
        resolve(&mut keyboard, "vim", &escape, seat),
        Dispatch::Action("escape")
    );
    assert_eq!(resolve(&mut keyboard, "vim", &y, seat), Dispatch::Armed);
    assert!(keyboard.pair_armed("vim", seat));
    assert!(keyboard.pair_armed("vim", seat), "a read keeps the arm");
    assert!(!keyboard.pair_armed("vim", Seat { cursor: 1, ..seat }));
    assert!(!keyboard.pair_armed("default", seat));
    keyboard.context = Context::Rail;
    assert!(!keyboard.pair_armed("vim", seat));
    keyboard.context = Context::Listing;
    assert_eq!(
        resolve(&mut keyboard, "vim", &escape, seat),
        Dispatch::Action("escape")
    );
    assert!(!keyboard.pair_armed("vim", seat));
    assert_eq!(resolve(&mut keyboard, "vim", &y, seat), Dispatch::Armed);
}

#[test]
fn keypad_bit_reaches_rows_and_paste_but_not_the_digit_rule() {
    // The keypad flag rides outside MODIFIER_MASK, as Qt's KeypadModifier does in v0.3.12.
    assert_eq!(action("default", Context::Listing, "", "1", 0), "tab1");
    assert_eq!(action("default", Context::Listing, "", "1", KEYPAD), "");
    assert_eq!(
        action("default", Context::Listing, "Return", "", KEYPAD),
        "open"
    );
    assert_eq!(
        action("default", Context::Listing, "Enter", "", KEYPAD),
        "open"
    );
    let paste = |key, modifiers| {
        input::paste_input(Input {
            key,
            text: "",
            modifiers,
        })
    };
    assert!(
        paste("insert", SHIFT | KEYPAD),
        "Shift with numpad 0 under NumLock"
    );
    assert!(paste("insert", SHIFT));
    assert!(paste("v", CONTROL | KEYPAD));
    assert!(!paste("insert", KEYPAD));
    assert!(!paste("insert", SHIFT | ALT | KEYPAD));
    assert!(!paste("v", CONTROL | SHIFT | KEYPAD));
    let mut keyboard = Keyboard::default();
    for (key, text, mask, expected) in [
        ("1", Some("1"), 0, Dispatch::Action("tab1")),
        ("1", Some("1"), KEYPAD, Dispatch::Unbound),
        ("9", Some("9"), KEYPAD, Dispatch::Unbound),
        ("enter", None, KEYPAD, Dispatch::Action("open")),
    ] {
        assert_eq!(
            resolve(
                &mut keyboard,
                "default",
                &event(key, text, mask),
                Seat::default()
            ),
            expected,
            "{key} {mask}"
        );
    }
    assert!(input::paste_key(&event("insert", None, SHIFT | KEYPAD)));
}
