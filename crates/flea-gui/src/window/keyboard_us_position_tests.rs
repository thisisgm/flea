use super::{event_input, event_lookup, position_input};
use crate::g2::listing_filter::{
    filter::LineKey,
    keys::{classify, Classified},
};
use flea::keys::{lookup, Context, Frontend, Match, PRESETS};
use gpui::{KeyDownEvent, Keystroke, Modifiers};

// A key press as fork 0298abf6 reports it: `key` is the US key on a non-Latin group and the printed key on a Latin one.
fn press(
    key: &str,
    typed: Option<&str>,
    us: Option<&str>,
    held: Modifiers,
) -> (KeyDownEvent, Modifiers) {
    let caseless = key.to_lowercase() == key.to_uppercase();
    let event = KeyDownEvent {
        keystroke: Keystroke {
            modifiers: Modifiers {
                shift: held.shift && !caseless,
                ..held
            },
            key: key.into(),
            key_char: typed.map(Into::into),
            is_keypad: false,
            us_key: us.map(Into::into),
        },
        is_held: false,
        prefer_character_input: false,
    };
    (event, held)
}

fn action(context: Context, (event, held): (KeyDownEvent, Modifiers)) -> Match {
    event_lookup("default", context, &event, held, false)
}

fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Default::default()
    }
}

#[test]
fn nonlatin_text_bindings_take_the_us_position() {
    // Russian, Hebrew and Arabic type their own letter at the US t position.
    for typed in ["е", "א", "ف"] {
        let t = press("t", Some(typed), Some("t"), Modifiers::none());
        assert_eq!(
            action(Context::Listing, t),
            Match::Action("tabNew"),
            "{typed}"
        );
    }
    // Ctrl+t types nothing, and the fork already names the key t on every group.
    assert_eq!(
        action(
            Context::Listing,
            press("t", None, Some("t"), Modifiers::control())
        ),
        Match::Action("openTerminal")
    );
    // Arabic Shift at the US slash types the Arabic question mark; Russian types a comma there.
    assert_eq!(
        action(Context::Listing, press("?", Some("؟"), Some("?"), shift())),
        Match::Action("keymapSheet")
    );
    assert_eq!(
        action(Context::Listing, press("?", Some(","), Some("?"), shift())),
        Match::Action("keymapSheet")
    );
}

#[test]
fn nonlatin_typing_keeps_the_typed_character() {
    let (event, held) = press("t", Some("е"), Some("t"), Modifiers::none());
    assert_eq!(event_input(&event, held).text, "е");
    // The search query line and the filter line both classify through sr2.
    assert_eq!(
        classify("default", &event, held),
        Classified::Key(LineKey::Text("е"))
    );
    let (event, held) = press("?", Some("؟"), Some("?"), shift());
    assert_eq!(
        classify("default", &event, held),
        Classified::Key(LineKey::Text("؟"))
    );
}

#[test]
fn digits_take_the_us_position_first() {
    // AZERTY prints & on the 1 key and - on the 6 key.
    assert_eq!(
        action(
            Context::Listing,
            press("&", Some("&"), Some("1"), Modifiers::none())
        ),
        Match::Action("tab1")
    );
    assert_eq!(
        action(
            Context::Listing,
            press("&", None, Some("1"), Modifiers::control())
        ),
        Match::Action("viewList")
    );
    // - is bound in the listing (zoomOut), but the digit position wins there; the preview, which binds no 6, keeps -.
    assert_eq!(
        action(
            Context::Listing,
            press("-", Some("-"), Some("6"), Modifiers::none())
        ),
        Match::Action("tab6")
    );
    assert_eq!(
        action(
            Context::Preview,
            press("-", Some("-"), Some("6"), Modifiers::none())
        ),
        Match::Action("zoomOut")
    );
    // Rule b: the listing's zoomOut answers at the US - position (AE11), where AZERTY prints an unbound ).
    assert_eq!(
        action(
            Context::Listing,
            press(")", Some(")"), Some("-"), Modifiers::none())
        ),
        Match::Action("zoomOut")
    );
    // Ctrl+Shift+0 is the 0 key on every layout: AZERTY types 0 with Shift, US types ).
    let ctrl_shift = Modifiers {
        control: true,
        shift: true,
        ..Default::default()
    };
    assert_eq!(
        action(Context::Listing, press("0", None, Some(")"), ctrl_shift)),
        Match::Action("textSizeReset")
    );
    assert_eq!(
        action(Context::Listing, press(")", None, Some(")"), ctrl_shift)),
        Match::Action("textSizeReset")
    );
    // Shift stays an explicit modifier: AZERTY Shift+1 types 1 but is no bare digit.
    assert_eq!(
        action(Context::Listing, press("1", Some("1"), Some("!"), shift())),
        Match::Unbound
    );
}

#[test]
fn latin_letters_match_only_where_printed() {
    // AZERTY prints a at the US q position.
    let (event, held) = press("a", Some("a"), Some("q"), Modifiers::none());
    assert!(position_input(&event, held).is_none());
    let (event, held) = press("t", Some("t"), Some("t"), Modifiers::none());
    assert!(position_input(&event, held).is_none());
    // Without the fork's position the lookup is the printed key alone.
    let (event, held) = press("&", Some("&"), None, Modifiers::none());
    assert!(position_input(&event, held).is_none());
    assert_eq!(event_input(&event, held).text, "&");
}

#[test]
fn latin_ascii_letters_at_us_punctuation_keys_match_only_where_printed() {
    // Dvorak prints z at the US slash key, Colemak o and AZERTY m at the US semicolon key.
    let letters = [
        press("z", Some("Z"), Some("?"), shift()),
        press("o", Some("O"), Some(":"), shift()),
        press("m", Some("m"), Some(";"), Modifiers::none()),
        press("m", None, Some(";"), Modifiers::control()),
    ];
    for (event, held) in letters {
        assert!(
            position_input(&event, held).is_none(),
            "{:?}",
            event.keystroke
        );
        for preset in PRESETS {
            for context in Context::ALL {
                assert_eq!(
                    event_lookup(preset, context, &event, held, false),
                    lookup(preset, context, Frontend::Gui, event_input(&event, held)),
                    "{preset} {context:?} {:?}",
                    event.keystroke
                );
            }
        }
    }
    // Before the fix these ran keymapSheet and pathBar through the US key under the letter.
    assert_eq!(
        action(Context::Listing, press("z", Some("Z"), Some("?"), shift())),
        Match::Unbound
    );
    assert_eq!(
        action(Context::Listing, press("o", Some("O"), Some(":"), shift())),
        Match::Unbound
    );
    // Rule b puts Ctrl+= on the US = key, where Italian prints ì, so that letter keeps its US position.
    assert_eq!(
        action(
            Context::Listing,
            press("igrave", Some("ì"), Some("="), Modifiers::control())
        ),
        Match::Action("textSizeUp")
    );
    // AZERTY's é at the US 2 key is no ASCII letter, so the key is still its digit.
    assert_eq!(
        action(
            Context::Listing,
            press("eacute", Some("é"), Some("2"), Modifiers::none())
        ),
        Match::Action("tab2")
    );
}

#[test]
fn punctuation_takes_the_printed_key_first_only_on_a_latin_group() {
    // AZERTY types : at the US period key; rule a runs the colon binding, not the period's.
    assert_eq!(
        action(
            Context::Listing,
            press(":", Some(":"), Some("."), Modifiers::none())
        ),
        Match::Action("pathBar")
    );
    // Russian types . at the US slash key; rule c runs the slash binding, not the printed period's.
    assert_eq!(
        action(
            Context::Listing,
            press("/", Some("."), Some("/"), Modifiers::none())
        ),
        Match::Action("filter")
    );
}

#[test]
fn every_command_lookup_reads_the_us_position() {
    // Typing classifiers keep printed keys; Quick Look resolves through the keyboard's own route (EY-438).
    let sites = [
        ("rail_keys.rs", include_str!("rail_keys.rs")),
        ("network_menu.rs", include_str!("network_menu.rs")),
        (
            "trash_browser/join_input.rs",
            include_str!("../g2/trash_browser/join_input.rs"),
        ),
    ];
    // Keyboard::lookup reads the US position and leaves AltGr text unbound, as resolve does.
    for (name, source) in sites {
        let source: String = source.split_whitespace().collect();
        let routed = source.matches("self.keyboard.lookup(").count();
        assert!(routed > 0, "{name} no longer looks a key up here");
        assert_eq!(
            source.matches("lookup(").count(),
            routed,
            "{name} looks a key up without its US position or the AltGr check"
        );
    }
}

#[test]
fn altgr_presses_never_take_the_us_position() {
    // The AE11 press that answers zoomOut by position stays text while AltGr is held.
    let (mut event, held) = press(")", Some(")"), Some("-"), Modifiers::none());
    event.prefer_character_input = true;
    assert!(position_input(&event, held).is_none());
    assert_eq!(action(Context::Listing, (event, held)), Match::Unbound);
}

#[test]
fn ey346_a_partial_keymap_letter_matches_where_printed() {
    // wtype uploads a keymap holding only the keys it sends, so the fork names y at keycode 10 by its US key 1.
    let y = press("1", Some("y"), Some("1"), Modifiers::none());
    assert!(position_input(&y.0, y.1).is_none());
    assert_eq!(event_input(&y.0, y.1).key, "y");
    assert_eq!(action(Context::Listing, y), Match::Action("copy"));
    assert_eq!(
        action(
            Context::Listing,
            press("1", Some("x"), Some("1"), Modifiers::none())
        ),
        Match::Action("cut")
    );
    // A Russian letter at the US 1 position still never reaches this path, and AZERTY's 1 stays tab1.
    assert_eq!(
        action(
            Context::Listing,
            press("&", Some("&"), Some("1"), Modifiers::none())
        ),
        Match::Action("tab1")
    );
}

#[test]
fn adv_f2_shifted_partial_map_letters_keep_text_and_key_bindings() {
    for (context, letter, action_name) in [
        (Context::Listing, "Y", "copydirpath"),
        (Context::Preview, "F", "previewPageForward"),
    ] {
        let partial = press("1", Some(letter), Some("1"), shift());
        let printed = letter.to_ascii_lowercase();
        let complete = press(&printed, Some(letter), Some(&printed), shift());
        assert_eq!(action(context, complete), Match::Action(action_name));
        assert_eq!(action(context, partial), Match::Action(action_name));
    }
}
