// Review r3's findings as tests: the check restarts a full interval after the Trash view's busy, and a replaced request is dropped, never said.
use super::host::Overlay;
use super::model::{Dialog, Operations};
use super::palette::CardPalette;
use super::tests::trash_source;
use flea::i18n::I18n;
use flea::ui_logic_file_ops::{confirm::Source, trash};
use gpui::TestAppContext;
use std::sync::Arc;
use std::time::Duration;

fn review(token: &str) -> trash::Review {
    trash::Review {
        count: Some(2),
        token: token.into(),
        ..Default::default()
    }
}

const CHECK: &str = r#"{"c":"trashbrowse","op":"check","id":1,"token":4}"#;

// A Trash card whose check is out.
fn checking(cat: &I18n) -> Operations {
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), 0);
    ops.tick(cat, 2500);
    assert_eq!(ops.outbox, vec![CHECK]);
    ops
}

// A Trash card whose check found the Trash changed, so a fresh review is out and no card is drawn.
fn re_reviewing(cat: &I18n) -> Operations {
    let mut ops = checking(cat);
    ops.line(
        cat,
        r#"{"t":"trashbrowse","id":1,"op":"check","ok":true,"valid":false}"#,
        2600,
    );
    assert!(ops.dialog.is_none() && ops.trash_request_out());
    ops
}

// r3-1, review PROOF1 turned: busy over 1000..4000 ms sends nothing 1 ms after it clears and the check 2500 ms after.
#[gpui::test]
fn the_check_restarts_a_full_interval_when_the_trash_view_frees(cx: &mut TestAppContext) {
    let (overlay, vcx) = cx.add_window_view(|window, cx| {
        window.set_rem_size(gpui::px(14.));
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = CardPalette::from_theme(&window.text_style(), theme, 8.);
        Overlay::new(
            Arc::new(crate::test_catalogs::english()),
            palette,
            "default",
        )
    });
    overlay.update(vcx, |o, cx| {
        o.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), cx)
    });
    vcx.executor().advance_clock(Duration::from_millis(1000));
    vcx.run_until_parked();
    overlay.update(vcx, |o, cx| o.trash_busy(true, cx));
    vcx.executor().advance_clock(Duration::from_millis(3000));
    vcx.run_until_parked();
    overlay.update(vcx, |o, cx| o.trash_busy(false, cx));
    vcx.executor().advance_clock(Duration::from_millis(1));
    vcx.run_until_parked();
    assert!(overlay.read_with(vcx, |o, _| o.model.outbox.is_empty()));
    vcx.executor().advance_clock(Duration::from_millis(2499));
    vcx.run_until_parked();
    assert_eq!(
        overlay.read_with(vcx, |o, _| o.model.outbox.clone()),
        vec![CHECK]
    );
}

// r3-2, review PROOF2 turned: a check the Trash view's request replaced keeps the card, says nothing and checks again later.
#[test]
fn a_replaced_check_keeps_the_card_and_says_nothing() {
    let cat = crate::test_catalogs::english();
    let mut ops = checking(&cat);
    ops.line(
        &cat,
        r#"{"t":"trashbrowse","id":1,"op":"check","ok":false,"stale":false,"error":"Trash request cancelled."}"#,
        2600,
    );
    assert!(matches!(ops.dialog, Some(Dialog::Delete { .. })) && ops.confirm.is_some());
    assert!(ops.said.is_empty(), "{:?}", ops.said);
    assert!(!ops.trash_request_out());
    assert_eq!(ops.due(), Some(5100));
}

// r3-2: on the shared counter a later request id marks the reply replaced, whatever its sentence.
#[test]
fn a_reply_behind_the_shared_counter_is_dropped() {
    let cat = crate::test_catalogs::english();
    let mut ops = checking(&cat);
    ops.requests += 1;
    ops.line(
        &cat,
        r#"{"t":"trashbrowse","id":1,"op":"check","ok":false,"stale":false,"error":"boom"}"#,
        2600,
    );
    assert!(matches!(ops.dialog, Some(Dialog::Delete { .. })) && ops.said.is_empty());
}

// r3-2 with r4-2: a replaced fresh review ends the Trash confirmation quietly and sends nothing, the view's request already superseded it.
#[test]
fn a_replaced_review_ends_quietly() {
    let cat = crate::test_catalogs::english();
    let mut ops = re_reviewing(&cat);
    let before = ops.outbox.len();
    ops.line(
        &cat,
        r#"{"t":"trashbrowse","id":2,"op":"prepare","ok":false,"stale":false,"error":"Trash request cancelled."}"#,
        2700,
    );
    assert!(ops.confirm.is_none() && ops.dialog.is_none() && ops.said.is_empty());
    assert_eq!(ops.outbox.len(), before, "{:?}", ops.outbox);
}

// r3-2, review PROOF3 turned: a menu refresh a new menu snapshot cancelled is dropped quietly and its snapshot closed.
#[test]
fn a_replaced_menu_refresh_is_dropped_quietly() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("3"), trash::Scope::Items, Source::Menu(6), 0);
    ops.tick(&cat, 2500);
    ops.line(
        &cat,
        r#"{"t":"menuaction","id":6,"op":"checkDelete","ok":true,"valid":false}"#,
        2600,
    );
    ops.line(
        &cat,
        r#"{"t":"menuaction","id":6,"op":"refreshDelete","ok":false,"error":"Menu request cancelled.","cancelled":true}"#,
        2700,
    );
    assert!(ops.said.is_empty(), "{:?}", ops.said);
    assert!(ops.confirm.is_none() && ops.dialog.is_none());
    assert_eq!(
        ops.outbox.last().map(String::as_str),
        Some(r#"{"c":"menuaction","op":"close","id":6}"#)
    );
}

// r3-2: a real refusal is still said; only a replaced request is dropped.
#[test]
fn a_real_refusal_is_still_said() {
    let cat = crate::test_catalogs::english();
    let mut ops = checking(&cat);
    ops.line(
        &cat,
        r#"{"t":"trashbrowse","id":1,"op":"check","ok":false,"stale":false,"error":"Trash worker unavailable."}"#,
        2600,
    );
    assert!(ops.confirm.is_none() && ops.dialog.is_none());
    assert_eq!(
        ops.said.last().map(|s| (s.text.as_str(), s.error)),
        Some(("Trash worker unavailable.", true))
    );
}

// r3-2 seam: the Trash view closing during a fresh review ends it, so the review landing later is never drawn.
#[test]
fn the_trash_view_closing_ends_a_review_out() {
    let cat = crate::test_catalogs::english();
    let mut ops = re_reviewing(&cat);
    ops.trash_closed();
    assert!(ops.confirm.is_none() && !ops.trash_request_out());
    ops.line(
        &cat,
        r#"{"t":"trashbrowse","id":2,"op":"prepare","ok":true,"token":5,"all":false,"count":2,"bytes":10,"partial":false}"#,
        2700,
    );
    assert!(ops.dialog.is_none() && ops.said.is_empty());
}

// r3-2 seam: a delete already out outlives the Trash view, so its Empty Trash progress still lands.
#[test]
fn a_delete_out_outlives_the_trash_view() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), 0);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    assert!(ops.trash_request_out());
    ops.trash_closed();
    ops.line(
        &cat,
        r#"{"t":"trashdeletestarted","id":40,"requestId":1}"#,
        0,
    );
    assert_eq!(ops.emptying.id, 40);
}
