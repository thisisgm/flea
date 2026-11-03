//! Control alone with Equal, Plus, Minus or 0 sizes the listing text, as the Shift chords do, in every preset.
use crate::window::input::event_input;
use flea::keys::{self, Context, Frontend, Match};
use gpui::{KeyDownEvent, Keystroke};

fn press(stroke: &str) -> KeyDownEvent {
    let mut keystroke = Keystroke::parse(stroke).unwrap();
    // A bare symbol types itself, as the platform reports it.
    if stroke.chars().count() == 1 {
        keystroke.key_char = Some(stroke.into());
    }
    KeyDownEvent {
        keystroke,
        is_held: false,
        prefer_character_input: false,
    }
}

fn action(preset: &str, context: Context, stroke: &str) -> Match {
    let event = press(stroke);
    let held = event.keystroke.modifiers;
    keys::lookup(preset, context, Frontend::Gui, event_input(&event, held))
}

#[test]
fn control_alone_sizes_listing_text_and_shift_chords_stay() {
    for preset in ["default", "vim", "mac", "windows"] {
        for (stroke, want) in [
            ("ctrl-=", "textSizeUp"),
            ("ctrl-+", "textSizeUp"),
            ("ctrl--", "textSizeDown"),
            ("ctrl-0", "textSizeReset"),
            ("ctrl-shift-=", "textSizeUp"),
            ("ctrl-shift-+", "textSizeUp"),
            ("ctrl-shift--", "textSizeDown"),
            ("ctrl-shift-0", "textSizeReset"),
        ] {
            assert_eq!(
                action(preset, Context::Listing, stroke),
                Match::Action(want),
                "{preset} {stroke}"
            );
        }
    }
}

#[test]
fn bare_plus_and_minus_still_zoom_a_preview() {
    assert_eq!(
        action("default", Context::Pdf, "+"),
        Match::Action("zoomIn")
    );
    assert_eq!(
        action("default", Context::Pdf, "-"),
        Match::Action("zoomOut")
    );
    assert_ne!(
        action("default", Context::Pdf, "ctrl-="),
        Match::Action("textSizeUp")
    );
}
