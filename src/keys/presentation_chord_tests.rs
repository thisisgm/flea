//! The menu's chord hints: Print… shows the ctrl-p the window answers, and no other row gains a chord.
use super::hint_for;
use crate::keys::{lookup, Context, Frontend, Input, CONTROL, PRESETS};

const CTRL_P: Input<'static> = Input {
    key: "P",
    text: "",
    modifiers: CONTROL,
};

#[test]
fn print_hints_the_ctrl_p_the_window_answers_in_every_preset() {
    for preset in PRESETS {
        assert_eq!(
            lookup(preset, Context::Listing, Frontend::Gui, CTRL_P).action(),
            "print",
            "{preset}: the window reads ctrl-p from the key table"
        );
        assert_eq!(
            hint_for(preset, "print"),
            "ctrl-p",
            "{preset}: Print… shows ctrl-p"
        );
    }
}

#[test]
fn ctrl_p_prints_only_in_the_listing() {
    for context in [Context::Rail, Context::Menu, Context::Preview] {
        assert_ne!(
            lookup("default", context, Frontend::Gui, CTRL_P).action(),
            "print",
            "{}",
            context.name()
        );
    }
}

#[test]
fn rows_bound_only_by_a_ctrl_chord_keep_no_hint() {
    let chord_only = [
        "duplicate",
        "eject",
        "openTab",
        "properties",
        "selectAll",
        "newFolder",
        "openTerminal",
    ];
    for preset in PRESETS {
        for action in chord_only {
            assert_eq!(hint_for(preset, action), "", "{preset} {action}");
        }
    }
}
