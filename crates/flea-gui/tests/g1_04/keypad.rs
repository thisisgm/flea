use super::dispatch::event;
use super::*;
use flea::keys::{CONTROL, KEYPAD, SHIFT, SUPER};
use flea_gui::window::input;
use Context::{Listing, Pdf, Preview, Rail};

// The fork's keypad name, the character it types and the Qt key v0.3.12 reads for that keysym.
const OPERATORS: [(&str, &str, &str); 7] = [
    ("add", "+", "Plus"),
    ("subtract", "-", "Minus"),
    ("multiply", "*", "Asterisk"),
    ("divide", "/", "Slash"),
    ("separator", ",", "Comma"),
    ("decimal", ".", "Period"),
    ("equal", "=", "Equal"),
];

const VIEWS: &[Context] = &[Listing, Rail, Preview, Pdf];
const PANES: &[Context] = &[Listing, Rail];

// Every non-empty action v0.3.12's Keymap.js gives a keypad operator with KeypadModifier set, plus PR #302's Control-only text size chords; any other answer is none.
const ANSWERS: [(&str, &str, u8, &str, &[Context]); 19] = [
    ("all", "add", 0, "zoomIn", VIEWS),
    ("all", "add", SHIFT, "zoomIn", VIEWS),
    ("all", "add", CONTROL | SHIFT, "textSizeUp", PANES),
    ("all", "subtract", 0, "zoomOut", VIEWS),
    ("all", "subtract", SHIFT, "zoomOut", VIEWS),
    ("all", "subtract", CONTROL | SHIFT, "textSizeDown", PANES),
    ("all", "divide", 0, "filter", PANES),
    ("all", "divide", SHIFT, "filter", PANES),
    ("all", "separator", 0, "settings", PANES),
    ("all", "separator", CONTROL, "settings", PANES),
    ("all", "separator", SHIFT, "settings", PANES),
    ("all", "decimal", 0, "toggleHidden", PANES),
    ("all", "decimal", SHIFT, "toggleHidden", PANES),
    ("all", "decimal", CONTROL | SHIFT, "toggleHidden", PANES),
    ("mac", "decimal", SUPER | SHIFT, "toggleHidden", &[Listing]),
    ("all", "equal", CONTROL | SHIFT, "textSizeUp", PANES),
    ("all", "add", CONTROL, "textSizeUp", PANES),
    ("all", "subtract", CONTROL, "textSizeDown", PANES),
    ("all", "equal", CONTROL, "textSizeUp", PANES),
];

fn fork(preset: &str, context: Context, key: &str, text: &str, mask: u8) -> Match {
    let event = event(key, Some(text), mask);
    let input = input::event_input(&event, event.keystroke.modifiers);
    keys::lookup(preset, context, Frontend::Gui, input)
}

fn v0_3_12(preset: &str, context: Context, key: &str, mask: u8) -> &'static str {
    ANSWERS
        .iter()
        .find(|row| {
            (row.0 == "all" || row.0 == preset)
                && row.1 == key
                && row.2 == mask
                && row.4.contains(&context)
        })
        .map_or("", |row| row.3)
}

#[test]
fn keypad_operators_resolve_as_the_qt_keys_v0_3_12_reads() {
    for preset in keys::PRESETS {
        for context in Context::ALL {
            for (key, text, qt_key) in OPERATORS {
                for mask in 0..16 {
                    let qt = keys::lookup(
                        preset,
                        context,
                        Frontend::Gui,
                        Input {
                            key: qt_key,
                            text,
                            modifiers: mask | KEYPAD,
                        },
                    );
                    let actual = fork(preset, context, key, text, mask | KEYPAD);
                    assert_eq!(actual, qt, "{preset} {context:?} {key} {mask}");
                    assert_eq!(
                        actual.action(),
                        v0_3_12(preset, context, key, mask),
                        "{preset} {context:?} {key} {mask}"
                    );
                }
            }
        }
    }
    for preset in keys::PRESETS {
        for (key, text, mask, expected) in [
            ("add", "+", CONTROL | SHIFT, "textSizeUp"),
            ("subtract", "-", CONTROL | SHIFT, "textSizeDown"),
            ("separator", ",", CONTROL, "settings"),
        ] {
            assert_eq!(
                fork(preset, Listing, key, text, mask | KEYPAD),
                Match::Action(expected),
                "{preset} {key} {mask}"
            );
        }
    }
}

#[test]
fn a_keypad_name_from_another_key_is_not_remapped() {
    // No layout names a main-block key "add"; "multiply" is the keysym name of U+00D7, which the fork can send.
    for (key, text) in [("add", "+"), ("multiply", "\u{d7}")] {
        for mask in [0, CONTROL, CONTROL | SHIFT] {
            let plain = event(key, Some(text), mask);
            assert_eq!(
                input::event_input(&plain, plain.keystroke.modifiers).key,
                key,
                "{key} {mask}"
            );
        }
    }
    let plain = event("add", Some("+"), CONTROL | SHIFT);
    let input = input::event_input(&plain, plain.keystroke.modifiers);
    assert_eq!(
        keys::lookup("default", Listing, Frontend::Gui, input),
        Match::Unbound
    );
    let pad = event("add", Some("+"), CONTROL | SHIFT | KEYPAD);
    assert_eq!(
        input::event_input(&pad, pad.keystroke.modifiers).key,
        "Plus"
    );
}
