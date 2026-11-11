//! The compress and extract cards as the boards draw them: strings, focus seats and every press.
use super::cards::{Block, CheckLine, Chips, Ink};
use super::compress::{Compress, Stop as CompressStop};
use super::keys::Key;
use super::tests::{buttons, english, fields, notes, said, sent};
use super::Effect;

fn compress() -> Compress {
    Compress::open(
        vec!["/fx/v/work/notes.txt".into()],
        "/fx/v/work",
        "work",
        0,
        &["7z", "zip"],
    )
    .0
}

fn typed(card: &mut Compress, i18n: &flea::i18n::I18n, secret: &str) {
    for c in secret.chars() {
        card.key(i18n, Key::Text(c.to_string()));
    }
}

#[test]
fn compress_opens_with_the_boards_order_and_password_focused() {
    let i18n = english();
    let card = compress();
    let drawn = card.card(&i18n);
    assert_eq!(
        (drawn.title.as_str(), drawn.width),
        ("Compress 1 Item", 590.)
    );
    let Block::Chips(Chips {
        options,
        current,
        focused,
        label,
    }) = &drawn.body[0]
    else {
        panic!("chips first")
    };
    assert_eq!(
        (options.join("|").as_str(), *current, *focused),
        ("7z|zip", 0, false)
    );
    assert_eq!(label.as_str(), "Format");
    assert_eq!(
        fields(&drawn),
        vec![
            ("Name".into(), "notes.7z".into(), false, false),
            ("Password".into(), String::new(), true, false),
            ("Confirm".into(), String::new(), false, false),
        ]
    );
    assert_eq!(buttons(&drawn), "Cancel|Compress*");
}

#[test]
fn compress_submit_reaches_the_wire_and_clears_the_pair() {
    let i18n = english();
    let mut card = compress();
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Tab);
    typed(&mut card, &i18n, "s3cret");
    let effects = card.press(&i18n, CompressStop::Compress);
    let lines = sent(&effects);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains(r#""password":"s3cret""#), "{}", lines[0]);
    assert!(lines[0].contains(r#""encryptNames":true"#), "{}", lines[0]);
    assert_eq!(
        said(&effects),
        vec![("Compressing 1 item to .7z".to_string(), false)]
    );
    assert!(!card.opened());
    assert_eq!(card.password_len(), 0);
}

#[test]
fn compress_formats_rename_the_name_and_mask_header_encryption() {
    let i18n = english();
    let mut card = compress();
    card.key(&i18n, Key::Backtab);
    card.key(&i18n, Key::Backtab);
    card.key(&i18n, Key::Right);
    let drawn = card.card(&i18n);
    let Block::Chips(Chips {
        options,
        current,
        focused,
        ..
    }) = &drawn.body[0]
    else {
        panic!("chips")
    };
    assert_eq!((*current, options.len(), *focused), (1, 2, true));
    assert_eq!(fields(&drawn)[0].1, "notes.zip");
    card.key(&i18n, Key::Left);
    let drawn = card.card(&i18n);
    assert_eq!(fields(&drawn)[0].1, "notes.7z");
    assert!(notes(&drawn).is_empty(), "no sentence yet");
}

#[test]
fn compress_zip_submit_sends_header_encryption_off() {
    let i18n = english();
    let mut card = compress();
    card.key(&i18n, Key::Backtab);
    card.key(&i18n, Key::Backtab);
    card.key(&i18n, Key::Right);
    card.key(&i18n, Key::Tab);
    card.key(&i18n, Key::Tab);
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Tab);
    typed(&mut card, &i18n, "s3cret");
    let effects = card.press(&i18n, CompressStop::Compress);
    let lines = sent(&effects);
    assert!(lines[0].contains(r#""format":"zip""#), "{}", lines[0]);
    assert!(lines[0].contains(r#""encryptNames":false"#), "{}", lines[0]);
}

#[test]
fn compress_empty_and_mismatched_pairs_ask_in_sentences() {
    let i18n = english();
    let mut card = compress();
    let effects = card.press(&i18n, CompressStop::Compress);
    assert!(sent(&effects).is_empty());
    assert_eq!(
        notes(&card.card(&i18n)),
        vec![("Type a password first.".into(), Ink::Error)]
    );
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Tab);
    typed(&mut card, &i18n, "other");
    let effects = card.press(&i18n, CompressStop::Compress);
    assert!(sent(&effects).is_empty());
    assert_eq!(
        notes(&card.card(&i18n)),
        vec![("Passwords do not match.".into(), Ink::Error)]
    );
    assert!(card.opened());
}

#[test]
fn compress_empty_name_drops_the_sentence_and_focuses_name() {
    let i18n = english();
    let mut card = compress();
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Tab);
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Backtab);
    card.key(&i18n, Key::Backtab);
    for _ in 0.."notes.7z".len() {
        card.key(&i18n, Key::Backspace);
    }
    let effects = card.press(&i18n, CompressStop::Compress);
    assert!(sent(&effects).is_empty(), "an empty name sends nothing");
    let drawn = card.card(&i18n);
    assert!(notes(&drawn).is_empty(), "the old sentence drops");
    assert!(fields(&drawn)[0].2, "focus lands on Name");
    assert!(card.opened(), "the card stays open");
}

#[test]
fn compress_encrypt_switch_answers_only_on_7z() {
    let i18n = english();
    let mut card = compress();
    let drawn = card.card(&i18n);
    assert!(
        matches!(&drawn.body[4], Block::CheckLine(CheckLine { checked: true, label, .. }) if label == "Encrypt file names")
    );
    card.key(&i18n, Key::Backtab);
    card.key(&i18n, Key::Backtab);
    card.key(&i18n, Key::Right);
    let drawn = card.card(&i18n);
    assert!(!drawn
        .body
        .iter()
        .any(|block| matches!(block, Block::CheckLine(_))));
}

#[test]
fn compress_pointer_entries_answer_without_the_key_walk() {
    let i18n = english();
    let mut card = compress();
    card.pick_format(1);
    let drawn = card.card(&i18n);
    assert_eq!(fields(&drawn)[0].1, "notes.zip");
    assert!(
        !drawn
            .body
            .iter()
            .any(|block| matches!(block, Block::CheckLine(_))),
        "zip hides the header switch"
    );
    card.pick_format(0);
    let before = card.card(&i18n);
    assert!(before
        .body
        .iter()
        .any(|block| matches!(block, Block::CheckLine(_))));
    card.toggle_encrypt();
    let after = card.card(&i18n);
    assert!(
        after
            .body
            .iter()
            .any(|block| matches!(block, Block::CheckLine(CheckLine { checked: false, .. }))),
        "a tap flips the switch"
    );
    card.focus(CompressStop::Cancel);
    assert_eq!(
        card.key(&i18n, Key::Enter),
        vec![Effect::Close],
        "a focused Cancel closes on Enter"
    );
}

#[test]
fn compress_close_clears_without_sending() {
    let i18n = english();
    let mut card = compress();
    typed(&mut card, &i18n, "s3cret");
    let effects = card.key(&i18n, Key::Escape);
    assert_eq!(effects, vec![Effect::Close]);
    assert!(!card.opened());
    assert_eq!(card.password_len(), 0);
    assert!(sent(&card.press(&i18n, CompressStop::Compress)).is_empty());
}

#[test]
fn compress_enter_on_focused_cancel_closes_without_sending() {
    let i18n = english();
    let mut card = compress();
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Tab);
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Tab);
    card.key(&i18n, Key::Tab);
    let effects = card.key(&i18n, Key::Enter);
    assert_eq!(
        effects,
        vec![Effect::Close],
        "Enter on Cancel closes, never commits"
    );
    assert!(!card.opened());
    assert_eq!(card.password_len(), 0);
}

#[test]
fn compress_space_on_focused_cancel_closes_without_sending() {
    let i18n = english();
    let mut card = compress();
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Tab);
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Tab);
    card.key(&i18n, Key::Tab);
    let effects = card.key(&i18n, Key::Space);
    assert_eq!(
        effects,
        vec![Effect::Close],
        "Space on Cancel closes, never commits"
    );
    assert!(!card.opened());
}

#[test]
fn compress_space_on_focused_compress_commits() {
    let i18n = english();
    let mut card = compress();
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Tab);
    typed(&mut card, &i18n, "s3cret");
    card.key(&i18n, Key::Tab);
    card.key(&i18n, Key::Tab);
    card.key(&i18n, Key::Tab);
    let effects = card.key(&i18n, Key::Space);
    assert_eq!(
        sent(&effects).len(),
        1,
        "Space on Compress commits like Enter"
    );
}

#[path = "card_secret_tests.rs"]
mod secrets;
