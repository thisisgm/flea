use super::*;
use flea::keys::{sequence::Seat, ALT, CONTROL, KEYPAD, SHIFT, SUPER};
use flea_gui::window::input::{self, Dispatch, Keyboard};
use gpui::{KeyDownEvent, Keystroke, Modifiers};

pub(super) fn event(key: &str, text: Option<&str>, mask: u8) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke {
            key: key.into(),
            key_char: text.map(str::to_owned),
            modifiers: Modifiers {
                control: mask & CONTROL != 0,
                shift: mask & SHIFT != 0,
                alt: mask & ALT != 0,
                platform: mask & SUPER != 0,
                ..Default::default()
            },
            is_keypad: mask & KEYPAD != 0,
            us_key: None,
        },
        is_held: false,
        prefer_character_input: false,
    }
}

pub(super) fn resolve(
    keyboard: &mut Keyboard,
    name: &str,
    input: &KeyDownEvent,
    seat: Seat,
) -> Dispatch {
    keyboard.resolve(name, input, input.keystroke.modifiers, false, seat)
}

#[test]
fn presets_contexts_masks_and_empty_overrides() {
    assert_eq!(action("mac", Context::Listing, "N", "", CONTROL), "");
    assert_eq!(action("mac", Context::Listing, "T", "", CONTROL), "tabNew");
    assert_eq!(
        action("default", Context::Listing, "N", "", CONTROL),
        "windowNew"
    );
    for preset in keys::PRESETS {
        let mask = if preset == "windows" {
            CONTROL | SHIFT
        } else {
            CONTROL
        };
        for (digit, view) in [
            ("1", "viewList"),
            ("2", "viewColumns"),
            ("3", "viewGrid"),
            ("4", "viewDual"),
        ] {
            assert_eq!(action(preset, Context::Listing, digit, "", mask), view);
            assert_eq!(action(preset, Context::Editor, digit, "", mask), "");
        }
        for context in Context::ALL {
            assert_eq!(action(preset, context, "Q", "q", CONTROL), "");
            for mask in 0..16 {
                let actual = action(preset, context, "F10", "", mask);
                let expected =
                    if mask == SHIFT && matches!(context, Context::Listing | Context::Rail) {
                        "menu"
                    } else {
                        ""
                    };
                assert_eq!(actual, expected, "{preset} {context:?} {mask}");
            }
        }
    }
    assert_eq!(
        keys::lookup(
            "mac",
            Context::Listing,
            Frontend::Gui,
            Input {
                key: "X",
                text: "x",
                modifiers: CONTROL
            }
        ),
        Match::Suppressed
    );
    assert_eq!(action("mac", Context::Listing, "Return", "", 0), "rename");
    assert_eq!(
        action("mac", Context::Listing, "Backspace", "", 0),
        "parent"
    );
    assert_eq!(
        action("windows", Context::Listing, "H", "", CONTROL),
        "toggleHidden"
    );
    assert_eq!(
        action("unknown", Context::Listing, "1", "", CONTROL),
        "viewList"
    );
    assert_eq!(action("default", Context::Pdf, "J", "j", 0), "");
    assert_eq!(action("default", Context::Media, "J", "j", 0), "cursorDown");
    assert_eq!(action("default", Context::Media, "P", "p", 0), "playPause");
    assert_eq!(action("default", Context::Menu, "1", "1", 0), "tag1");
    assert_eq!(action("default", Context::Rail, "2", "2", 0), "tab2");
    assert_eq!(
        action("default", Context::Editor, "Escape", "", 0),
        "escape"
    );
}

#[test]
fn text_authority_and_native_composition_boundaries() {
    let mut keyboard = Keyboard::default();
    let seat = Seat::default();
    for (key, text, mask, expected) in [
        ("g", Some("g"), 0, Dispatch::Action("cursorFirst")),
        ("g", Some("G"), SHIFT, Dispatch::Action("cursorLast")),
        ("g", Some("\u{03b3}"), 0, Dispatch::Unbound),
        ("g", None, 0, Dispatch::Unbound),
        ("d", Some("d"), CONTROL, Dispatch::Action("pageDown")),
        ("d", Some("d"), CONTROL | SHIFT, Dispatch::Unbound),
        ("j", Some("j"), ALT, Dispatch::Unbound),
        ("j", Some("j"), CONTROL | ALT, Dispatch::Unbound),
        ("enter", None, 0, Dispatch::Action("open")),
        ("space", Some(" "), 0, Dispatch::Action("preview")),
    ] {
        assert_eq!(
            resolve(&mut keyboard, "default", &event(key, text, mask), seat),
            expected
        );
    }
    keyboard.set_composing(true);
    assert_eq!(
        resolve(&mut keyboard, "default", &event("j", Some("j"), 0), seat),
        Dispatch::Composition
    );
    keyboard.set_composing(false);
    let mut altgr = event("j", Some("j"), CONTROL | ALT);
    altgr.prefer_character_input = true;
    assert_eq!(
        resolve(&mut keyboard, "default", &altgr, seat),
        Dispatch::Composition
    );
    assert_eq!(
        resolve(&mut keyboard, "default", &event("j", Some("j"), 0), seat),
        Dispatch::Action("cursorDown")
    );
    assert!(input::printable("\u{754c}"));
    assert!(!input::printable("\u{7f}"));
    assert!(!input::printable("ab"));
    assert!(!input::printable("\u{20000}"));
    assert_eq!(
        input::clean_paste("a\tb\u{20000}\u{7f}\r\nsecond"),
        "ab\u{20000}"
    );
    assert!(input::paste_key(&event("v", None, CONTROL)));
    assert!(input::paste_key(&event("insert", None, SHIFT)));
    assert!(!input::paste_key(&event("v", None, CONTROL | SHIFT)));
    let mut pasted = String::new();
    assert!(input::paste_into("one\nsecond", |text| pasted.push_str(text)));
    assert_eq!(pasted, "one");
    assert!(input::paste_into("\nsecond", |_| panic!(
        "empty paste called typing"
    )));
}

#[test]
fn pairs_disarm_on_identity_context_preset_and_input_changes() {
    let mut keyboard = Keyboard::default();
    let initial = Seat::default();
    for (key, expected) in [
        ("y", "copy"),
        ("d", "cut"),
        ("p", "paste"),
        ("g", "cursorFirst"),
    ] {
        let input = event(key, Some(key), 0);
        assert_eq!(
            resolve(&mut keyboard, "vim", &input, initial),
            Dispatch::Armed
        );
        assert_eq!(
            resolve(&mut keyboard, "vim", &input, initial),
            Dispatch::Action(expected)
        );
    }
    let y = event("y", Some("y"), 0);
    let seats = [
        Seat {
            listing: 1,
            ..initial
        },
        Seat {
            cursor: 1,
            ..initial
        },
        Seat {
            selection: 1,
            ..initial
        },
        Seat { view: 1, ..initial },
        Seat {
            focus: 1,
            ..initial
        },
    ];
    for changed in seats {
        keyboard.disarm();
        assert_eq!(resolve(&mut keyboard, "vim", &y, initial), Dispatch::Armed);
        assert_eq!(resolve(&mut keyboard, "vim", &y, changed), Dispatch::Armed);
    }
    keyboard.disarm();
    assert_eq!(resolve(&mut keyboard, "vim", &y, initial), Dispatch::Armed);
    assert_eq!(
        resolve(&mut keyboard, "default", &y, initial),
        Dispatch::Action("copy")
    );
    assert_eq!(resolve(&mut keyboard, "vim", &y, initial), Dispatch::Armed);
    assert_eq!(
        resolve(&mut keyboard, "vim", &event("q", Some("q"), 0), initial),
        Dispatch::Unbound
    );
    assert_eq!(resolve(&mut keyboard, "vim", &y, initial), Dispatch::Armed);
    let mut repeated = y.clone();
    repeated.is_held = true;
    assert_eq!(
        resolve(&mut keyboard, "vim", &repeated, initial),
        Dispatch::Action("copy")
    );
    keyboard.context = Context::Editor;
    assert_eq!(
        resolve(&mut keyboard, "vim", &y, initial),
        Dispatch::Unbound
    );
    keyboard.context = Context::Listing;
    assert_eq!(resolve(&mut keyboard, "vim", &y, initial), Dispatch::Armed);
    assert_eq!(
        resolve(&mut keyboard, "vim", &event("x", Some("x"), 0), initial),
        Dispatch::Action("cut")
    );
    assert_eq!(
        resolve(&mut keyboard, "default", &event("d", Some("d"), 0), initial),
        Dispatch::Action("trashArm")
    );
    keyboard.disarm();
    assert_eq!(resolve(&mut keyboard, "vim", &y, initial), Dispatch::Armed);
    keyboard.set_composing(true);
    keyboard.set_composing(false);
    assert_eq!(resolve(&mut keyboard, "vim", &y, initial), Dispatch::Armed);
}

#[test]
fn pointer_rows_and_digits_are_generated_data() {
    for row in keys::POINTER {
        let found = keys::pointer(row.place, row.row, row.button, row.mask, row.taps).unwrap();
        assert_eq!((found.effect, found.press), (row.effect, row.press));
        let button = match row.button {
            "left" => gpui::MouseButton::Left,
            "right" => gpui::MouseButton::Right,
            "middle" => gpui::MouseButton::Middle,
            "back" => gpui::MouseButton::Navigate(gpui::NavigationDirection::Back),
            "forward" => gpui::MouseButton::Navigate(gpui::NavigationDirection::Forward),
            _ => panic!("unknown generated button"),
        };
        let press = gpui::MouseDownEvent {
            button,
            click_count: row.taps as usize,
            modifiers: event("", None, row.mask).keystroke.modifiers,
            ..Default::default()
        };
        assert_eq!(
            input::pointer(row.place, row.row, &press).unwrap().effect,
            row.effect
        );
    }
    assert_eq!(
        keys::pointer("listing", "result", "left", 0, 1)
            .unwrap()
            .effect,
        "reveal"
    );
    assert_eq!(
        keys::pointer("listing", "file", "left", 0, 2)
            .unwrap()
            .effect,
        "open"
    );
    assert!(keys::pointer("listing", "dir", "left", ALT, 1).is_none());
    for digit in keys::DIGITS {
        assert_eq!(
            action("default", Context::Listing, digit.text, digit.text, 0),
            digit.action
        );
        assert_eq!(
            action("default", Context::Listing, digit.text, digit.text, SHIFT),
            ""
        );
    }
    assert_eq!(action("default", Context::Listing, "0", "0", 0), "");
    assert_eq!(action("default", Context::Listing, "F3", "", 0), "");
    assert_eq!(
        action("default", Context::Listing, "F4", "", 0),
        "autofitColumns"
    );
}
