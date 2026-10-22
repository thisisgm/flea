//! The card's keys: the order is field, Cancel, Unlock, and a button takes Return, Enter and Space but not a held repeat.
use super::keys::{act, Act, Key};
use super::unlock::{Card, Part};

fn key(name: &str) -> Key<'_> {
    Key {
        name,
        text: None,
        shift: false,
        plain: true,
        held: false,
    }
}

fn card_on(part: Part) -> Card {
    let mut card = Card::new();
    card.open_for("/dev/sdb1", "Backup", 0, "crypto_LUKS", "2");
    card.type_text("a");
    card.focus = Some(part);
    card
}

#[test]
fn return_enter_and_space_press_the_focused_button() {
    for name in ["enter", "return", "space"] {
        assert_eq!(
            act(&card_on(Part::Cancel), key(name)),
            Act::Close,
            "{name} on Cancel"
        );
        assert_eq!(
            act(&card_on(Part::Unlock), key(name)),
            Act::Submit,
            "{name} on Unlock"
        );
    }
    assert_eq!(
        act(&card_on(Part::Field), key("enter")),
        Act::Submit,
        "Enter in the field submits"
    );
}

#[test]
fn a_held_key_never_presses_a_button_twice() {
    let held = Key {
        held: true,
        ..key("enter")
    };
    assert_eq!(act(&card_on(Part::Unlock), held), Act::Ignore);
    assert_eq!(
        act(
            &card_on(Part::Cancel),
            Key {
                held: true,
                ..key("space")
            }
        ),
        Act::Ignore
    );
}

#[test]
fn space_in_the_field_types_and_escape_always_closes() {
    let space = Key {
        text: Some(" "),
        ..key("space")
    };
    assert_eq!(act(&card_on(Part::Field), space), Act::Type(" ".into()));
    for part in [Part::Field, Part::Cancel, Part::Unlock] {
        assert_eq!(act(&card_on(part), key("escape")), Act::Close);
    }
}

#[test]
fn tab_steps_and_typing_lands_only_in_the_field() {
    assert_eq!(
        act(
            &card_on(Part::Field),
            Key {
                shift: true,
                ..key("tab")
            }
        ),
        Act::Tab { back: true }
    );
    let letter = Key {
        text: Some("x"),
        ..key("x")
    };
    assert_eq!(act(&card_on(Part::Field), letter), Act::Type("x".into()));
    assert_eq!(act(&card_on(Part::Cancel), letter), Act::Ignore);
    assert_eq!(
        act(
            &card_on(Part::Field),
            Key {
                plain: false,
                ..letter
            }
        ),
        Act::Ignore,
        "a shortcut is not text"
    );
    assert_eq!(act(&card_on(Part::Cancel), key("backspace")), Act::Ignore);
}
