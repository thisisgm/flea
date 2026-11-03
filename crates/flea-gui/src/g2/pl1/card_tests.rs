//! The card's words against the unlock and unlock-empty boards, through the real catalog and its omarchy references.
use super::card::CardText;
use super::process::Outcome;
use super::unlock::{Card, Proc};
use flea::i18n::{I18n, Paths};
use std::fs;
use std::path::{Path, PathBuf};

const MARKER: &str = ".flea-test-sandbox";

/// A marked scratch root under this unit's own .superpowers directory, removed only when the marker is there.
struct Root(PathBuf);

impl Root {
    fn new(tag: &str) -> Self {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g2-pl1/scratch");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root = base.join(format!("{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(MARKER), "").unwrap();
        Self(root)
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        if self.0.is_absolute() && self.0.join(MARKER).is_file() && self.0.components().count() > 4
        {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

/// English with the three omarchy words the device prefix borrows, as Omarchy ships them.
fn catalog(root: &Root) -> I18n {
    let omarchy = root.0.join("omarchy");
    fs::create_dir_all(omarchy.join("default/i18n/en")).unwrap();
    fs::write(omarchy.join("default/i18n/locales.json"), r#"{"en":{}}"#).unwrap();
    fs::write(
        omarchy.join("default/i18n/en/shell.json"),
        r#"{"network.passphrase":"Passphrase","common.cancel":"Cancel","dock.open":"Open"}"#,
    )
    .unwrap();
    fs::write(root.0.join("language"), "en\n").unwrap();
    I18n::load(Paths {
        omarchy,
        language_file: root.0.join("language"),
        catalogs: vec![root.0.join("catalogs")],
    })
}

fn board_card() -> Card {
    let mut card = Card::new();
    card.open_for("/dev/sdb1", "sdb1", 0, "crypto_LUKS", "");
    card
}

#[test]
fn unlock_empty_board_words() {
    let root = Root::new("card-empty");
    let text = CardText::of(&board_card(), &catalog(&root));
    assert_eq!(text.title, "Unlock sdb1");
    assert_eq!(text.esc, "esc");
    assert_eq!(text.subtitle, "sdb1 \u{b7} LUKS");
    assert_eq!(text.label, "PASSPHRASE");
    assert_eq!(text.note, "", "the line under the field is held empty");
    assert_eq!(text.cancel, "Cancel");
    assert_eq!(text.unlock, "Unlock");
    assert!(!board_card().unlock_available(), "Unlock draws disabled");
}

#[test]
fn unlock_wrong_board_words_and_seven_held_dots() {
    let root = Root::new("card-wrong");
    let mut card = board_card();
    card.type_text("1234567");
    card.submit(0);
    let refused = Outcome {
        code: 1,
        stdout: String::new(),
        stderr: "GDBus.Error:org.freedesktop.UDisks2.Error.Failed: x".into(),
    };
    card.exited(Proc::Unlock, &refused, 5);
    let text = CardText::of(&card, &catalog(&root));
    assert_eq!(text.note, "That passphrase did not work. Check Caps Lock.");
    assert_eq!(card.field.dots(), 7);
    assert!(card.field.error && card.unlock_available());
}

#[test]
fn a_known_size_joins_the_drive_line() {
    let root = Root::new("card-size");
    let mut card = Card::new();
    card.open_for("/dev/sdb1", "Backup", 500_000_000_000, "crypto_LUKS", "2");
    let text = CardText::of(&card, &catalog(&root));
    assert_eq!(text.subtitle, "Backup \u{b7} 500.0 GB \u{b7} LUKS2");
}

#[test]
fn the_pointer_washes_are_the_oracle_dialog_buttons() {
    // v0.3.12 ui/js/Buttons.js:14-16: hover 8%, press 14%, the primary rests at the press weight.
    assert_eq!(super::card::WASH_HOVER, 0.08);
    assert_eq!(super::card::WASH_PRESS, 0.14);
}

/// The nine roles read through the card, each a different theme ink so a wash or frame taken from the wrong role shows.
fn distinct_ink() -> super::ink::CardInk {
    let theme = crate::palette::Theme::default();
    let ink = super::ink::CardInk {
        radius: 0.,
        surface: theme.background,
        ground: theme.background.opacity(0.9),
        foreground: theme.foreground,
        muted: theme.foreground.opacity(0.62),
        frame: theme.foreground.opacity(0.5),
        accent_frame: theme.foreground.opacity(0.7),
        error: theme.foreground.opacity(0.8),
        rule: theme.foreground.opacity(0.4),
        wash: theme.foreground.opacity(0.33),
    };
    assert_pairwise_distinct(&ink);
    ink
}

fn assert_pairwise_distinct(ink: &super::ink::CardInk) {
    let roles = [
        ("surface", ink.surface),
        ("ground", ink.ground),
        ("foreground", ink.foreground),
        ("muted", ink.muted),
        ("frame", ink.frame),
        ("accent_frame", ink.accent_frame),
        ("error", ink.error),
        ("rule", ink.rule),
        ("wash", ink.wash),
    ];
    for (i, (one, first)) in roles.iter().enumerate() {
        for (other, second) in &roles[i + 1..] {
            assert_ne!(
                first, second,
                "card ink roles must be pairwise distinct: {one} and {other}"
            );
        }
    }
}

#[test]
#[should_panic(expected = "pairwise distinct")]
fn the_inherited_ink_collapses_roles_so_the_helper_rejects_it() {
    let palette = crate::list::Palette::inherited(
        &gpui::TextStyle::default(),
        crate::palette::Theme::default(),
    );
    assert_pairwise_distinct(&super::ink::CardInk::inherited(&palette));
}

#[test]
fn a_hovered_or_pressed_unlock_keeps_its_accent_wash_under_the_pointer_wash() {
    use super::card::{button_wash, WASH_HOVER, WASH_PRESS};
    let ink = distinct_ink();
    let unlock = button_wash(true, true, &ink);
    assert_eq!(
        unlock.rest,
        Some(ink.wash),
        "the primary rests on the accent wash in every pointer state"
    );
    assert_eq!(unlock.hover, Some(ink.foreground.opacity(WASH_HOVER)));
    assert_eq!(unlock.press, Some(ink.foreground.opacity(WASH_PRESS)));
    let cancel = button_wash(false, true, &ink);
    assert_eq!(cancel.rest, None, "a secondary has no resting wash");
    assert_eq!(
        (cancel.hover, cancel.press),
        (unlock.hover, unlock.press),
        "both buttons take the same pointer washes"
    );
    let unavailable = button_wash(true, false, &ink);
    assert_eq!(
        unavailable,
        super::card::ButtonWash {
            rest: None,
            hover: None,
            press: None
        },
        "unavailable Unlock draws no wash and answers no pointer"
    );
}

#[test]
fn a_focused_primary_on_a_hued_accent_keeps_one_foreground_hairline() {
    use super::card::button_frame;
    let mut ink = distinct_ink();
    assert_ne!(ink.accent_frame, ink.foreground);
    assert_eq!(
        button_frame(true, true, true, &ink),
        (ink.foreground, false),
        "Buttons.focusFrame with a hued accent: one foreground hairline"
    );
    assert_eq!(
        button_frame(true, true, false, &ink),
        (ink.accent_frame, false),
        "the resting primary keeps its accent frame"
    );
    ink.accent_frame = ink.foreground;
    assert_eq!(
        button_frame(true, true, true, &ink),
        (ink.foreground, true),
        "RING wide only where the rest frame is the foreground"
    );
}
