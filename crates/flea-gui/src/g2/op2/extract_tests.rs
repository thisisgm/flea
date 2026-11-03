//! The extract card as the boards draw it: strings, focus seats and every press.
use super::cards::Ink;
use super::extract::{Extract, Stop as ExtractStop};
use super::keys::Key;
use super::tests::{buttons, english, fields, notes, said, sent};
use super::Effect;

#[test]
fn extract_pointer_focus_moves_without_the_key_walk() {
    let i18n = english();
    let mut card = Extract::closed();
    card.open("/s/one.7z", "/s/one", "", &i18n);
    card.take_secret_for_test("s3cret");
    card.focus(ExtractStop::Extract);
    assert_eq!(
        sent(&card.key(&i18n, Key::Space)).len(),
        1,
        "a focused Extract commits on Space"
    );
}
#[test]
fn extract_opens_over_its_archive_with_extract_unavailable() {
    let i18n = english();
    let mut card = Extract::closed();
    card.open("/fx/v/work/locked.7z", "/fx/v/work/locked", "", &i18n);
    assert!(card.opened());
    assert_eq!(card.leaf(), "locked.7z");
    let drawn = card.card(&i18n);
    assert_eq!(drawn.title.as_str(), "locked.7z is encrypted.");
    assert_eq!(drawn.width, 588.);
    assert_eq!(buttons(&drawn), "Cancel|Extract*!");
}

#[test]
fn holding_the_eye_reveals_the_extract_field() {
    use super::cards::Block;
    let i18n = english();
    let mut card = Extract::closed();
    card.open("/fx/v/work/locked.7z", "/fx/v/work/locked", "", &i18n);
    card.take_secret_for_test("s3cret");
    assert!(fields(&card.card(&i18n))[0].2, "the field opens focused");
    assert_eq!(
        fields(&card.card(&i18n))[0].1,
        "••••••",
        "the field starts masked"
    );
    card.eye_press();
    let drawn = card.card(&i18n);
    let Block::Field(field) = &drawn.body[0] else {
        panic!("password field")
    };
    assert!(field.revealed, "the eye is held");
    assert_eq!(
        field.drawn(),
        "s3cret",
        "holding the eye shows the extract password"
    );
    card.eye_release();
    assert_eq!(
        fields(&card.card(&i18n))[0].1,
        "••••••",
        "releasing the eye masks it again"
    );
}

#[test]
fn extract_submit_reaches_the_wire_and_clears() {
    let i18n = english();
    let mut card = Extract::closed();
    card.open("/a/backup.7z", "/a/backup", "", &i18n);
    card.take_secret_for_test("s3cret");
    let effects = card.press(&i18n, ExtractStop::Extract);
    assert_eq!(sent(&effects), vec![r#"{"c":"archive","op":"extract","path":"/a/backup.7z","dest":"/a/backup","password":"s3cret","menuId":0}"#.to_string()]);
    assert!(!card.opened());
}

#[test]
fn extract_empty_field_asks_for_its_password() {
    let i18n = english();
    let mut card = Extract::closed();
    card.open("/s/one.7z", "/s/one", "", &i18n);
    let effects = card.press(&i18n, ExtractStop::Extract);
    assert!(sent(&effects).is_empty());
    assert_eq!(
        notes(&card.card(&i18n)),
        vec![("Type a password first.".into(), Ink::Error)]
    );
    assert!(card.opened());
}

#[test]
fn extract_wrong_answer_reopens_with_sentence_and_bullets() {
    let i18n = english();
    let mut card = Extract::closed();
    card.open("/s/one.7z", "/s/one", "", &i18n);
    card.take_secret_for_test("s3cret");
    assert!(card
        .press(&i18n, ExtractStop::Extract)
        .iter()
        .any(|e| matches!(e, Effect::Send(_))));
    card.open("/s/one.7z", "/s/one", "wrong-password", &i18n);
    assert_eq!(
        notes(&card.card(&i18n)),
        vec![("That password did not open one.7z.".into(), Ink::Error)]
    );
    assert_eq!(card.held_marks(), 6);
    let drawn = card.card(&i18n);
    assert_eq!(fields(&drawn)[0].1, "••••••");
    assert!(
        fields(&drawn)[0].3,
        "the refused field wears the error frame"
    );
    // Extract over the bullets alone sends nothing and asks for the password.
    let effects = card.press(&i18n, ExtractStop::Extract);
    assert!(sent(&effects).is_empty());
    assert_eq!(
        notes(&card.card(&i18n)),
        vec![("Type a password first.".into(), Ink::Error)]
    );
    // Typing again replaces the bullets and clears the error state.
    card.key(&i18n, Key::Text("x".into()));
    assert_eq!(card.held_marks(), 0);
    assert!(notes(&card.card(&i18n)).is_empty());
}

#[test]
fn extract_bullets_belong_to_the_cards_own_submit() {
    let i18n = english();
    let mut card = Extract::closed();
    card.open("/s/one.7z", "/s/one", "", &i18n);
    card.take_secret_for_test("s3cret");
    card.press(&i18n, ExtractStop::Extract);
    card.open("/s/other.7z", "/s/other", "wrong-password", &i18n);
    assert_eq!(
        card.held_marks(),
        0,
        "another archive's wrong password draws no bullets"
    );
    card.close();
    card.open("/s/one.7z", "/s/one", "encrypted", &i18n);
    assert_eq!(
        card.held_marks(),
        0,
        "a first ask of the same archive draws none"
    );
    card.take_secret_for_test("s3cret");
    card.press(&i18n, ExtractStop::Extract);
    card.open("/s/one.7z", "/s/one", "wrong-password", &i18n);
    assert_eq!(card.held_marks(), 6, "the re-ask after it draws the count");
    card.close();
    card.open("/s/one.7z", "/s/one", "wrong-password", &i18n);
    assert_eq!(
        card.held_marks(),
        0,
        "a closed re-ask spent the count, so a later one draws none"
    );
}
#[test]
fn extract_enter_on_focused_cancel_closes_without_sending() {
    let i18n = english();
    let mut card = Extract::closed();
    card.open("/s/one.7z", "/s/one", "", &i18n);
    card.take_secret_for_test("s3cret");
    card.key(&i18n, Key::Tab);
    let effects = card.key(&i18n, Key::Enter);
    assert_eq!(
        effects,
        vec![Effect::Close],
        "Enter on Cancel closes, never commits"
    );
    assert!(!card.opened());
}

#[test]
fn extract_space_on_focused_cancel_closes_without_sending() {
    let i18n = english();
    let mut card = Extract::closed();
    card.open("/s/one.7z", "/s/one", "", &i18n);
    card.take_secret_for_test("s3cret");
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
fn extract_space_on_focused_extract_commits() {
    let i18n = english();
    let mut card = Extract::closed();
    card.open("/s/one.7z", "/s/one", "", &i18n);
    card.take_secret_for_test("s3cret");
    card.key(&i18n, Key::Tab);
    card.key(&i18n, Key::Tab);
    let effects = card.key(&i18n, Key::Space);
    assert_eq!(
        sent(&effects).len(),
        1,
        "Space on Extract commits like Enter"
    );
}

#[test]
fn extract_second_open_keeps_the_first_card() {
    let i18n = english();
    let mut card = Extract::closed();
    assert!(card.open("/s/one.7z", "/s/one", "", &i18n).is_empty());
    let effects = card.open("/s/two.7z", "/s/two", "encrypted", &i18n);
    assert_eq!(
        card.leaf(),
        "one.7z",
        "a second request keeps the first card"
    );
    assert_eq!(
        said(&effects),
        vec![(
            "Another archive is waiting for its password. Enter it, then try again.".to_string(),
            true
        )],
        "and is answered with a whole sentence"
    );
}

#[test]
fn compress_request_over_an_open_card_is_answered_too() {
    let i18n = english();
    let mut card = Extract::closed();
    card.open("/s/one.7z", "/s/one", "", &i18n);
    let busy = super::card_busy(false, card.opened(), &i18n);
    assert_eq!(
        busy.map(|e| said(&[e])),
        Some(vec![(
            "Another archive is waiting for its password. Enter it, then try again.".to_string(),
            true
        )]),
        "a compress request over an open card is answered too"
    );
    assert!(
        super::card_busy(false, false, &i18n).is_none(),
        "no card open answers nothing"
    );
}

#[test]
fn extracting_job_block_carries_head_file_bytes_and_cancel() {
    use flea::ui_logic_op2::wire::ExtractSample;
    let i18n = english();
    let block = Extract::job_block(
        &i18n,
        &ExtractSample {
            id: 9,
            done: 6,
            total: 18,
            file: "DSC_4907.jpg".into(),
            item_bytes: 586600,
            bytes: 3700000,
            total_bytes: 9700000,
            rate: 2400000,
            left: Some(2),
        },
    );
    assert_eq!(
        (block.id, block.head.as_str(), block.file.as_str()),
        (9, "Extracting 7 of 18", "DSC_4907.jpg · 586.6 kB")
    );
    let text: String = block.byte.iter().map(|(text, _)| text.clone()).collect();
    assert_eq!(text, "3.7 MB of 9.7 MB · 2.4 MB/s · 0:02 left");
    assert!(block.byte.first().is_some_and(|(_, figure)| *figure));
    assert!(block.cancel, "Cancel stays live while entries land");
}
#[test]
fn extract_state_names_no_secret() {
    let i18n = english();
    let mut card = Extract::closed();
    card.open("/s/one.7z", "/s/one", "", &i18n);
    card.take_secret_for_test("S3cr3t-canary-1006");
    card.press(&i18n, ExtractStop::Extract);
    let dump = format!("{:?}", card.card(&i18n));
    assert!(
        !dump.contains("S3cr3t-canary"),
        "nothing holds the secret after submit"
    );
    card.open("/s/two.7z", "/s/two", "", &i18n);
    card.take_secret_for_test("S3cr3t-canary-1006");
    card.key(&i18n, Key::Escape);
    let dump = format!("{:?}", card.card(&i18n));
    assert!(
        !dump.contains("S3cr3t-canary"),
        "nothing holds the secret after close"
    );
}
