use super::{event_lookup, reading_input};
use flea::keys::{Context, Input, Match, CONTROL};
use gpui::{KeyDownEvent, Keystroke, Modifiers};

// A key press as the fork reports it on a Latin group (key is the printed key) or a non-Latin one (us_key names the position).
fn press(key: &str, typed: Option<&str>, us: Option<&str>, held: Modifiers) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke {
            modifiers: held,
            key: key.into(),
            key_char: typed.map(Into::into),
            is_keypad: false,
            us_key: us.map(Into::into),
        },
        is_held: false,
        prefer_character_input: false,
    }
}

fn action(context: Context, event: &KeyDownEvent, mirrored: bool) -> Match {
    event_lookup(
        "default",
        context,
        event,
        event.keystroke.modifiers,
        mirrored,
    )
}

#[test]
fn a_mirrored_window_reads_left_and_right_the_other_way_where_they_navigate() {
    let left = press("left", None, None, Modifiers::none());
    let right = press("right", None, None, Modifiers::none());
    let h = press("h", Some("h"), Some("h"), Modifiers::none());
    let l = press("l", Some("l"), Some("l"), Modifiers::none());
    // An unmirrored window, every left-to-right locale, reads h as before.
    assert_eq!(action(Context::Listing, &h, false), Match::Action("parent"));
    for (context, back, forward) in [
        (Context::Listing, "parent", "pageForward"),
        (Context::Rail, "parent", "pageForward"),
        (Context::Menu, "parent", "menuRight"),
    ] {
        assert_eq!(action(context, &left, false), Match::Action(back));
        assert_eq!(action(context, &right, false), Match::Action(forward));
        assert_eq!(
            action(context, &left, true),
            Match::Action(forward),
            "{context:?}"
        );
        assert_eq!(
            action(context, &right, true),
            Match::Action(back),
            "{context:?}"
        );
        assert_eq!(
            action(context, &h, true),
            Match::Action(forward),
            "{context:?}"
        );
        assert_eq!(
            action(context, &l, true),
            Match::Action(back),
            "{context:?}"
        );
    }
    // Hebrew types its own letter at the US h position; the position follows the same reading.
    let he_h = press("h", Some("י"), Some("h"), Modifiers::none());
    assert_eq!(
        action(Context::Listing, &he_h, true),
        Match::Action("pageForward")
    );
}

#[test]
fn mirrored_cards_swap_navigation_and_keep_typed_characters() {
    use crate::g2::{archive_flash, open_with_properties};
    let left = press("left", None, None, Modifiers::none());
    let right = press("right", None, None, Modifiers::none());
    let letter = press("h", Some("h"), Some("h"), Modifiers::none());
    assert_eq!(
        archive_flash::keys::key_of(&left).unwrap().read(true),
        archive_flash::keys::Key::Right
    );
    assert_eq!(
        archive_flash::keys::key_of(&right).unwrap().read(true),
        archive_flash::keys::Key::Left
    );
    assert_eq!(
        archive_flash::keys::key_of(&letter).unwrap().read(true),
        archive_flash::keys::Key::Text("h".into())
    );
    assert_eq!(
        open_with_properties::keys::key_of(&left)
            .unwrap()
            .read(true),
        open_with_properties::keys::Key::Right
    );
    assert_eq!(
        open_with_properties::keys::key_of(&right)
            .unwrap()
            .read(true),
        open_with_properties::keys::Key::Left
    );
    assert_eq!(
        open_with_properties::keys::key_of(&letter)
            .unwrap()
            .read(true),
        open_with_properties::keys::Key::Text("h".into())
    );
}

#[test]
fn file_content_text_and_chorded_letters_keep_their_keys_when_mirrored() {
    // Preview, PDF and media seek through content, which keeps its own direction.
    for context in [Context::Preview, Context::Pdf, Context::Media] {
        for key in ["left", "right", "h", "l"] {
            let event = press(
                key,
                (key.len() == 1).then_some(key),
                None,
                Modifiers::none(),
            );
            assert_eq!(
                action(context, &event, true),
                action(context, &event, false),
                "{context:?} {key}"
            );
        }
    }
    // Only a plain h and l are navigation twins; a chord on them keeps its letter.
    for key in ["h", "l"] {
        let input = reading_input(
            Input {
                key,
                text: key,
                modifiers: CONTROL,
            },
            true,
        );
        assert_eq!(
            (input.key, input.text, input.modifiers),
            (key, key, CONTROL)
        );
        let event = press(key, None, Some(key), Modifiers::control());
        assert_eq!(
            action(Context::Listing, &event, true),
            action(Context::Listing, &event, false),
            "ctrl+{key}"
        );
    }
    // Arrows read mirrored with any modifiers, as Omarchy's readingKey does.
    let shift_left = press("left", None, None, Modifiers::shift());
    let shift_right = press("right", None, None, Modifiers::shift());
    assert_eq!(
        action(Context::Listing, &shift_left, true),
        action(Context::Listing, &shift_right, false)
    );
}

#[test]
fn panel_and_rail_are_spatial_contexts_even_without_a_bound_arrow_action() {
    assert!(super::reads_mirrored(Context::Panel));
    assert!(super::reads_mirrored(Context::Rail));
    assert!(!super::reads_mirrored(Context::Preview));
}
