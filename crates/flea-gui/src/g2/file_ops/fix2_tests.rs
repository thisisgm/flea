// Review r2's findings as tests: a replaced FAT card frees the host, a review never outlives its card, and the check is armed and held by the host.
use super::host::Overlay;
use super::model::{Dialog, Operations};
use super::palette::CardPalette;
use super::tests::{request, trash_source};
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

fn overlay(cx: &mut TestAppContext) -> (gpui::Entity<Overlay>, &mut gpui::VisualTestContext) {
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
    vcx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    (overlay, vcx)
}

const FAT: &str = r#"{"t":"fatnames","id":1,"fstype":"exfat","volume":"128GB","renames":[{"old":"a:b","new":"a_b","d":false}]}"#;
const STALE: &str = r#"{"t":"trashbrowse","id":1,"op":"delete","ok":false,"stale":true,"error":"Trash changed; confirm again."}"#;
const FRESH: &str = r#"{"t":"trashbrowse","id":2,"op":"prepare","ok":true,"token":5,"all":false,"count":2,"bytes":10,"partial":false}"#;

fn checks_and_cancels(ops: &Operations) -> (usize, usize) {
    let count = |op: &str| ops.outbox.iter().filter(|l| l.contains(op)).count();
    (count(r#""op":"check""#), count(r#""op":"cancel""#))
}

// A Trash card whose delete went stale, so a fresh review is out and no card is drawn.
fn stale_review(cat: &I18n) -> Operations {
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), 0);
    ops.key(cat, "tab", false);
    ops.key(cat, "enter", false);
    ops.line(cat, STALE, 0);
    assert!(ops.dialog.is_none() && ops.confirm.is_some());
    ops
}

// r2-1: Delete permanently or Copy To over an open FAT card leaves the host free for the next paste.
#[test]
fn a_card_over_the_fat_card_leaves_the_host_free() {
    let cat = crate::test_catalogs::english();
    for replace in ["confirm_delete", "copy_to"] {
        let mut ops = Operations::new("default");
        assert!(ops.ask(&cat, &request(), &[], false));
        ops.line(&cat, FAT, 0);
        assert!(matches!(ops.dialog, Some(Dialog::Fat { .. })));
        if replace == "confirm_delete" {
            ops.confirm_delete(review("3"), trash::Scope::Items, Source::Menu(4), 0);
            ops.press_delete(trash::Button::Cancel);
        } else {
            ops.copy_to(4, false, "/x");
            ops.key(&cat, "escape", false);
        }
        assert_eq!(
            (ops.collide.state(), ops.collide.fat_opened),
            ("idle", false)
        );
        assert!(
            ops.ask(&cat, &request(), &[], false),
            "{replace}: {:?}",
            ops.said
        );
        assert!(ops.said.is_empty(), "{replace}: {:?}", ops.said);
    }
}

// r2-1 with r1-6: a late delete start over an open FAT card leaves the card and its host as they were.
#[test]
fn a_late_delete_start_leaves_the_fat_card_and_its_host() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.ask(&cat, &request(), &[], false);
    ops.line(&cat, FAT, 0);
    ops.line(
        &cat,
        r#"{"t":"trashdeletestarted","id":40,"requestId":1}"#,
        0,
    );
    assert!(matches!(ops.dialog, Some(Dialog::Fat { .. })));
    assert!(ops.collide.fat_opened && ops.emptying.id == 0);
}

// r2-2: Copy To opened while a fresh review is out ends that confirmation, so nothing checks or reappears later.
#[test]
fn a_card_opened_while_a_review_is_out_ends_the_confirmation() {
    let cat = crate::test_catalogs::english();
    let mut ops = stale_review(&cat);
    ops.copy_to(7, false, "/x");
    assert!(ops.confirm.is_none());
    assert_eq!(
        ops.outbox.last().map(String::as_str),
        Some(r#"{"c":"trashbrowse","op":"cancel","id":3}"#)
    );
    ops.line(&cat, FRESH, 100);
    ops.key(&cat, "escape", false);
    for round in 0..3u64 {
        ops.tick(&cat, 2600 + round * 2600);
    }
    assert_eq!(
        (ops.confirm.is_some(), ops.due(), ops.dialog.is_none()),
        (false, None, true)
    );
    assert_eq!(checks_and_cancels(&ops), (0, 1));
}

// r2-2: a fresh review that lands while any other card is drawn is cancelled, never shown later.
#[test]
fn a_review_landing_under_another_card_is_cancelled() {
    let cat = crate::test_catalogs::english();
    let mut ops = stale_review(&cat);
    ops.dialog = Some(Dialog::CopyTo {
        menu: 7,
        moving: false,
        path: "/x".into(),
        focus: super::model::CopyFocus::Field,
        busy: false,
        error: String::new(),
    });
    ops.line(&cat, FRESH, 100);
    assert!(ops.confirm.is_none() && matches!(ops.dialog, Some(Dialog::CopyTo { .. })));
    assert_eq!(checks_and_cancels(&ops), (0, 1));
}

// r2-2: a delete already out keeps its session under a new card, so its reply and Empty Trash progress still land.
#[test]
fn a_delete_out_survives_a_new_card() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), 0);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    ops.copy_to(7, false, "/x");
    assert!(ops.confirm.is_none() && ops.trash_held.is_some() && ops.trash_request_out());
    ops.line(
        &cat,
        r#"{"t":"trashdeletestarted","id":40,"requestId":1}"#,
        0,
    );
    assert_eq!(ops.emptying.id, 40);
    assert_eq!(checks_and_cancels(&ops), (0, 0));
}

// r2-4: the Overlay seam opens the card on its own clock and arms the 2.5 s check; no caller schedules.
#[gpui::test]
fn the_overlay_seam_arms_the_check(cx: &mut TestAppContext) {
    let (overlay, vcx) = overlay(cx);
    overlay.update(vcx, |o, cx| {
        o.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), cx)
    });
    vcx.executor().advance_clock(Duration::from_millis(2500));
    vcx.run_until_parked();
    let out = overlay.read_with(vcx, |o, _| o.model.outbox.clone());
    assert_eq!(
        out,
        vec![r#"{"c":"trashbrowse","op":"check","id":1,"token":4}"#]
    );
}

// r2-4 and r3-1: while the Trash view has its own request out the check waits, then a full 2500 ms interval runs before it goes out.
#[gpui::test]
fn the_check_waits_while_the_trash_view_is_busy(cx: &mut TestAppContext) {
    let (overlay, vcx) = overlay(cx);
    overlay.update(vcx, |o, cx| {
        o.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), cx);
        o.trash_busy(true, cx);
    });
    vcx.executor().advance_clock(Duration::from_millis(10_000));
    vcx.run_until_parked();
    assert!(overlay.read_with(vcx, |o, _| o.model.outbox.is_empty()));
    overlay.update(vcx, |o, cx| o.trash_busy(false, cx));
    vcx.executor().advance_clock(Duration::from_millis(1));
    vcx.run_until_parked();
    assert!(overlay.read_with(vcx, |o, _| o.model.outbox.is_empty()));
    vcx.executor().advance_clock(Duration::from_millis(2499));
    vcx.run_until_parked();
    let out = overlay.read_with(vcx, |o, _| o.model.outbox.clone());
    assert_eq!(
        out,
        vec![r#"{"c":"trashbrowse","op":"check","id":1,"token":4}"#]
    );
}

// r2-5: a partly failed menu delete says the count line with the backend's sentence as its detail.
#[test]
fn a_partial_menu_delete_keeps_the_backends_sentence() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("1"), trash::Scope::Items, Source::Menu(6), 0);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    ops.line(&cat, r#"{"t":"menuaction","id":6,"op":"delete","ok":true,"deleted":1,"failed":1,"cancelled":false,"error":"1 selected item could not be fully deleted: /d/a: boom.","remaining":["/d/a"]}"#, 0);
    let said = ops.said.last().expect("said");
    assert_eq!(
        (said.text.as_str(), said.error, said.detail.as_str()),
        (
            "Deleted 1 of 2 items · 1 failed",
            true,
            "1 selected item could not be fully deleted: /d/a: boom."
        )
    );
}
