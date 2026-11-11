//! Card passwords refuse composition, and batch rename cards act on committed text only.
use super::{bulk, draw, input, mark, ready};
use crate::g2::archive_flash::{compress::Compress, extract, keys::Key};
use crate::g2::file_ops::dialog_editor::Slot;
use gpui::{EntityInputHandler, TestAppContext, VisualTestContext};
use std::ops::Range;

#[test]
fn i50_dialog_password_private() {
    use crate::{
        g2::device_rail::{private_entry, secret::Secret},
        path::entry::Entry,
    };
    let mut secret = Secret::new();
    let mut entry = Entry::default();
    private_entry::begin(&mut entry, &secret);
    let value = "a\u{1d11e}猫";
    // A composition never reaches a password: no preedit, nothing stored.
    assert!(!private_entry::replace(
        &mut entry,
        &mut secret,
        None,
        value,
        true
    ));
    assert!(secret.is_empty() && entry.ime_marked().is_none() && !entry.composes());
    assert!(private_entry::replace(
        &mut entry,
        &mut secret,
        None,
        value,
        false
    ));
    assert!(secret.expose() == value.as_bytes());
    assert!(!entry.text.contains('a') && !entry.text.contains('猫'));
    assert_eq!(entry.ime_marked(), None);
    assert_eq!(entry.ime_selection().range, 4..4);
    let mut adjusted = None;
    assert!(entry.ime_text(&(0..4), &mut adjusted).is_none());
    assert!(adjusted.is_none());
    assert!(entry.copyable().is_none());
    assert!(!entry.ime_key("escape"));
    entry.select_all();
    assert!(private_entry::replace(
        &mut entry,
        &mut secret,
        None,
        &"q".repeat(2048),
        false
    ));
    assert_eq!(secret.chars(), 2048);
    assert!(!format!("{secret:?}").contains(&"q".repeat(20)));
    let mut adjusted = Some(Range { start: 7, end: 9 });
    assert!(entry.ime_text(&(0..20), &mut adjusted).is_none());
    assert_eq!(adjusted, Some(7..9));
}

#[gpui::test]
fn ime_batch_preview_acts_on_committed_text_only(cx: &mut TestAppContext) {
    let (shell, cx, _) = ready(cx);
    bulk(&shell, cx);
    mark(&shell, cx, "猫");
    let card = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |s, _| {
            let card = &s.rename.bulk.as_ref().unwrap().card;
            (card.find.clone(), s.rename.ime.entry.text.clone())
        })
    };
    assert_eq!(
        card(cx),
        (String::new(), "猫".into()),
        "the field paints the preedit and the preview waits"
    );
    input(cx, &shell, |s, w, cx| {
        s.replace_text_in_range(None, "猫", w, cx)
    });
    assert_eq!(card(cx), ("猫".into(), "猫".into()));
}

#[gpui::test]
fn ime_shell_turns_the_input_method_back_on_after_a_password_card(cx: &mut TestAppContext) {
    let (shell, cx, _) = ready(cx);
    let accepts =
        |cx: &mut VisualTestContext| input(cx, &shell, |s, w, cx| s.accepts_text_input(w, cx));
    assert!(accepts(cx), "with no card the window's own fields compose");
    input(cx, &shell, |s, _, _| {
        let catalog = &s.locale.catalog;
        s.operations
            .archive
            .extract
            .open("/files/a.7z", "/files", "", catalog);
    });
    draw(cx);
    assert!(!accepts(cx), "the extract password keeps it off");
    input(cx, &shell, |s, _, _| {
        let catalog = &s.locale.catalog;
        s.operations
            .archive
            .extract
            .press(catalog, extract::Stop::Cancel);
        s.sync_dialog_editor();
    });
    draw(cx);
    assert!(accepts(cx), "closing the card turns it back on");
    let slot = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |s, _| s.dialog_value().map(|(slot, _)| slot))
    };
    input(cx, &shell, |s, _, _| {
        let (card, _) = Compress::open(vec!["/files/a.txt".into()], "/files", "files", 3, &["7z"]);
        s.operations.archive.compress = Some(card);
    });
    draw(cx);
    assert_eq!(slot(cx), Some(Slot::ArchivePassword));
    assert!(!accepts(cx), "the compress password keeps it off");
    input(cx, &shell, |s, w, cx| {
        s.replace_text_in_range(None, "pw", w, cx);
        assert_eq!(s.dialog_value(), Some((Slot::ArchivePassword, "pw")));
        assert_eq!(
            s.rename.ime.entry.display(),
            "\u{2022}".repeat(2),
            "the field paints bullets"
        );
        assert_eq!(
            s.text_for_range(0..2, &mut None, w, cx),
            None,
            "the input method reads nothing"
        );
    });
    input(cx, &shell, |s, _, _| {
        let card = s.operations.archive.compress.as_mut().unwrap();
        card.key(&s.locale.catalog, Key::Backtab);
    });
    draw(cx);
    assert_eq!(slot(cx), Some(Slot::ArchiveName));
    assert!(accepts(cx), "Backtab to the archive name turns it back on");
    mark(&shell, cx, "かな");
    input(cx, &shell, |s, w, cx| {
        assert_eq!(s.rename.ime.owner, Some(Slot::ArchiveName));
        assert_eq!(s.marked_text_range(w, cx), Some(0..2));
        assert_eq!(
            s.rename.ime.entry.text, "かな",
            "the name never carries the password"
        );
    });
}
