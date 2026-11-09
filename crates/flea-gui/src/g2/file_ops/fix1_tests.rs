// Review r1's findings as tests: each card answers its own session, the host drives its own clock, and one frame moves every spiral.
use super::host::Overlay;
use super::model::{Dialog, Operations};
use super::palette::CardPalette;
use super::tests::{request, trash_source};
use flea::ui_logic_file_ops::{confirm::Source, trash};
use gpui::{KeyDownEvent, Keystroke, TestAppContext};
use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};

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

const COLLIDED: &str = r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes.txt","d":false,"i":""}],"mergeable":false}"#;

fn job(id: u64, done: u64) -> String {
    format!(
        r#"{{"t":"job","job":{{"id":{id},"kind":"copy","head":"Copying {done} of 9","file":"a","done":{done},"total":9,"state":"running","output":[]}},"unfinished":{id}}}"#
    )
}

// r1-1: even with the two sessions' counters equal, a dismissed Trash card is cancelled and a menu delete never reaches the Trash.
#[test]
fn a_menu_delete_goes_out_as_menuaction_and_a_dismissed_trash_card_cancels() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("1"), trash::Scope::TrashItems, trash_source(), 0);
    ops.key(&cat, "escape", false);
    assert_eq!(
        ops.outbox,
        vec![r#"{"c":"trashbrowse","op":"cancel","id":1}"#]
    );
    assert!(ops.confirm.is_none());
    ops.confirm_delete(review("1"), trash::Scope::Items, Source::Menu(1), 0);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    assert_eq!(
        ops.outbox[1],
        r#"{"c":"menuaction","op":"delete","id":1,"token":1}"#
    );
    assert!(ops
        .outbox
        .iter()
        .all(|l| !l.contains(r#""c":"trashbrowse","op":"delete""#)));
    ops.line(&cat, r#"{"t":"menuaction","id":1,"op":"delete","ok":true,"deleted":2,"failed":0,"cancelled":false,"error":"","remaining":[]}"#, 0);
    assert_eq!(ops.outbox[2], r#"{"c":"menuaction","op":"close","id":1}"#);
    assert_eq!(
        ops.said.last().map(|s| s.text.as_str()),
        Some("Deleted 2 of 2 items")
    );
}

#[test]
fn a_menu_card_dismissed_closes_its_snapshot() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("1"), trash::Scope::Items, Source::Menu(6), 0);
    ops.press(&cat, super::press::Press::Delete(trash::Button::Cancel));
    assert_eq!(
        ops.outbox,
        vec![r#"{"c":"menuaction","op":"close","id":6}"#]
    );
}

// r1-2: a stale delete asks a fresh review with the old token as refreshToken, the dead token is never resent, a refusal is said.
#[test]
fn a_stale_delete_asks_a_fresh_review_and_a_refusal_is_said() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), 0);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    assert!(!ops.line(&cat, r#"{"t":"trashbrowse","id":1,"op":"delete","ok":false,"stale":true,"error":"Trash changed; confirm again."}"#, 0));
    assert_eq!(
        ops.outbox[1],
        r#"{"all":false,"emptyTrash":false,"uris":["trash:///a"],"c":"trashbrowse","op":"prepare","id":2,"refreshToken":4}"#
    );
    ops.key(&cat, "enter", false);
    assert_eq!(ops.outbox.len(), 2);
    ops.line(&cat, r#"{"t":"trashbrowse","id":2,"op":"prepare","ok":true,"token":5,"all":false,"count":2,"bytes":10,"partial":false}"#, 0);
    let Some(Dialog::Delete { review, focus, .. }) = &ops.dialog else {
        panic!("the fresh review shows no card")
    };
    assert_eq!(
        (review.token.as_str(), *focus),
        ("5", trash::Button::Cancel)
    );
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    assert_eq!(
        ops.outbox[2],
        r#"{"c":"trashbrowse","op":"delete","id":3,"token":5}"#
    );
    ops.line(&cat, r#"{"t":"trashbrowse","id":3,"op":"delete","ok":false,"stale":false,"error":"An operation is already running."}"#, 0);
    assert_eq!(
        ops.said.last().map(|s| (s.text.as_str(), s.error)),
        Some(("An operation is already running.", true))
    );
    assert!(ops.confirm.is_none() && ops.dialog.is_none());
}

#[test]
fn an_open_card_checks_its_review_every_2500_ms_and_a_changed_one_is_reviewed_again() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), 0);
    assert_eq!(ops.due(), Some(2500));
    ops.tick(&cat, 2499);
    assert!(ops.outbox.is_empty());
    ops.tick(&cat, 2500);
    assert_eq!(
        ops.outbox,
        vec![r#"{"c":"trashbrowse","op":"check","id":1,"token":4}"#]
    );
    assert_eq!(ops.due(), None);
    ops.line(
        &cat,
        r#"{"t":"trashbrowse","id":1,"op":"check","ok":true,"valid":true}"#,
        2600,
    );
    assert_eq!(ops.due(), Some(5100));
    ops.tick(&cat, 5100);
    ops.line(
        &cat,
        r#"{"t":"trashbrowse","id":2,"op":"check","ok":true,"valid":false}"#,
        5200,
    );
    assert!(ops.dialog.is_none());
    assert!(
        ops.outbox[2].contains(r#""op":"prepare","id":3,"refreshToken":4"#),
        "{:?}",
        ops.outbox
    );
    let mut menu = Operations::new("default");
    menu.confirm_delete(review("8"), trash::Scope::Items, Source::Menu(5), 0);
    menu.tick(&cat, 2500);
    assert_eq!(
        menu.outbox,
        vec![r#"{"c":"menuaction","op":"checkDelete","id":5,"token":8}"#]
    );
}

// r1-3: the held items land on the host's own timer; nothing calls tick by hand.
#[gpui::test]
fn held_items_land_on_the_hosts_own_beat(cx: &mut TestAppContext) {
    let (overlay, vcx) = overlay(cx);
    overlay.update(vcx, |o, cx| {
        o.line(r#"{"t":"transferstarted","id":5,"n":3,"moving":false}"#, cx);
        for i in 0..3 {
            o.line(
                &format!(
                    r#"{{"t":"transferitem","id":5,"index":{i},"name":"f{i}","ok":true,"err":""}}"#
                ),
                cx,
            );
        }
    });
    vcx.run_until_parked();
    assert_eq!(overlay.read_with(vcx, |o, _| o.model.wire.transfer.done), 0);
    vcx.executor().advance_clock(Duration::from_millis(100));
    vcx.run_until_parked();
    let (done, held, sticky) = overlay.read_with(vcx, |o, _| {
        (
            o.model.wire.transfer.done,
            o.model.wire.held.len(),
            o.model.sticky.clone(),
        )
    });
    assert_eq!((done, held), (3, 0));
    assert!(sticky.contains('3'), "{sticky}");
}

#[gpui::test]
fn the_check_goes_out_on_the_hosts_own_timer(cx: &mut TestAppContext) {
    let (overlay, vcx) = overlay(cx);
    overlay.update(vcx, |o, cx| {
        let now = o.now(cx);
        o.model
            .confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), now);
        o.schedule(cx);
    });
    vcx.executor().advance_clock(Duration::from_millis(2500));
    vcx.run_until_parked();
    let out = overlay.read_with(vcx, |o, _| o.model.outbox.clone());
    assert_eq!(
        out,
        vec![r#"{"c":"trashbrowse","op":"check","id":1,"token":4}"#]
    );
}

fn six_jobs(overlay: &gpui::Entity<Overlay>, vcx: &mut gpui::VisualTestContext) {
    overlay.update(vcx, |o, cx| {
        for id in 1..=6u64 {
            o.line(&job(id, 1), cx);
        }
    });
    super::draw_fixture::settle_progress(vcx);
}

fn frames(
    overlay: &gpui::Entity<Overlay>,
    vcx: &mut gpui::VisualTestContext,
) -> (Vec<usize>, usize) {
    let notified = Rc::new(Cell::new(0usize));
    let seen = notified.clone();
    let _keep = vcx.update(|_, cx| cx.observe(overlay, move |_, _| seen.set(seen.get() + 1)));
    let mut asked = Vec::new();
    for _ in 0..4 {
        vcx.executor().advance_clock(Duration::from_millis(16));
        asked.push(vcx.update(|window, cx| window.simulate_next_frame(cx)));
        vcx.run_until_parked();
    }
    (asked, notified.get())
}

// r1-4: six crawling spirals ask one frame between them, and their blocks are built once per change, not per frame.
#[gpui::test]
fn six_spirals_ask_one_frame_and_blocks_build_once_per_change(cx: &mut TestAppContext) {
    let (overlay, vcx) = overlay(cx);
    six_jobs(&overlay, vcx);
    let (asked, _) = frames(&overlay, vcx);
    assert_eq!(asked, vec![1; 4]);
    assert_eq!(overlay.read_with(vcx, |o, _| o.model.blocks_built), 6);
    overlay.update(vcx, |o, cx| {
        o.line(&job(3, 2), cx);
    });
    vcx.run_until_parked();
    frames(&overlay, vcx);
    assert_eq!(overlay.read_with(vcx, |o, _| o.model.blocks_built), 7);
}

#[gpui::test]
fn reduced_motion_asks_no_frame_for_six_spirals(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (overlay, vcx) = overlay(cx);
    six_jobs(&overlay, vcx);
    assert_eq!(frames(&overlay, vcx), (vec![0; 4], 0));
}

// r1-5: Copy To validates through the menu snapshot and asks collisions about it; a relative path is refused out loud.
#[test]
fn copy_to_validates_through_the_menu_and_refuses_a_relative_path() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.copy_to(7, false, "work");
    ops.key(&cat, "enter", false);
    assert!(matches!(
        &ops.dialog,
        Some(Dialog::CopyTo { error, .. }) if error == "Enter a full folder path, starting with /."
    ));
    assert!(ops.said.is_empty(), "the refusal stays on the card");
    assert!(ops.open() && ops.outbox.is_empty());
    let mut ops = Operations::new("default");
    ops.copy_to(7, true, "/home/u/work");
    ops.key(&cat, "tab", false);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    assert_eq!(
        ops.outbox,
        vec![
            r#"{"c":"menuaction","op":"validate","id":7,"action":"moveTo","dest":"/home/u/work"}"#
        ]
    );
    ops.press(&cat, super::press::Press::CopyConfirm);
    assert_eq!(ops.outbox.len(), 1);
    ops.line(&cat, r#"{"t":"menuaction","id":7,"op":"validate","ok":true,"action":"moveTo","paths":["/a/x"],"dest":"/home/u/work"}"#, 0);
    assert_eq!(
        ops.outbox[1],
        r#"{"c":"collisions","id":1,"dest":"/home/u/work","menuId":7}"#
    );
    assert_eq!(ops.outbox[2], r#"{"c":"menuaction","op":"close","id":7}"#);
    assert!(!matches!(ops.dialog, Some(Dialog::CopyTo { .. })));
}

#[test]
fn a_refused_validate_keeps_the_card_and_says_why() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.copy_to(7, false, "/home/u/work");
    ops.key(&cat, "enter", false);
    ops.line(&cat, r#"{"t":"menuaction","id":7,"op":"validate","ok":false,"error":"The items changed; select them again."}"#, 0);
    assert!(matches!(
        &ops.dialog,
        Some(Dialog::CopyTo { busy: false, error, .. }) if error == "The items changed; select them again."
    ));
    assert!(ops.said.is_empty(), "the refusal stays on the card");
}

// r1-6: a late trashdeletestarted touches no card, and a card that replaces a collision card cancels it in the host.
#[test]
fn a_late_delete_start_leaves_the_collision_card_and_its_host() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("3"), trash::Scope::TrashItems, trash_source(), 0);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    assert!(ops.ask(&cat, &request(), &[], false));
    ops.line(&cat, COLLIDED, 0);
    ops.line(
        &cat,
        r#"{"t":"trashdeletestarted","id":40,"requestId":1}"#,
        0,
    );
    assert!(matches!(ops.dialog, Some(Dialog::Collide { .. })));
    assert!(ops.emptying.running);
    ops.copy_to(2, false, "/x");
    assert_eq!(ops.collide.state(), "idle");
    assert!(ops.ask(&cat, &request(), &[], false));
}

#[test]
fn a_delete_start_for_another_request_starts_nothing() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("3"), trash::Scope::TrashItems, trash_source(), 0);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    ops.line(
        &cat,
        r#"{"t":"trashdeletestarted","id":40,"requestId":9}"#,
        0,
    );
    assert!(!ops.emptying.running);
}

// r1-7
#[test]
fn the_whole_trash_review_names_its_button_empty_trash() {
    let cat = crate::test_catalogs::english();
    assert_eq!(cat.tr(trash::Button::Delete.label_key(true)), "Empty Trash");
    assert_eq!(cat.tr(trash::Button::Delete.label_key(false)), "Delete");
}

// r1-8
#[gpui::test]
fn control_alt_and_super_combinations_do_nothing_on_a_card(cx: &mut TestAppContext) {
    let (overlay, vcx) = overlay(cx);
    overlay.update(vcx, |o, _| {
        o.model
            .confirm_delete(review("2"), trash::Scope::TrashItems, trash_source(), 0)
    });
    for key in ["ctrl-tab", "ctrl-enter", "alt-enter", "super-enter"] {
        let event = KeyDownEvent {
            keystroke: Keystroke::parse(key).unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        overlay.update_in(vcx, |o, window, cx| o.key(&event, window, cx));
    }
    let (out, open) = overlay.read_with(vcx, |o, _| (o.model.outbox.clone(), o.model.open()));
    assert!(out.is_empty() && open, "{out:?}");
}

// r1-9: the field name is drawn with the board's 1.2px letter spacing after every letter.
#[gpui::test]
fn the_field_label_is_tracked_1_2px_per_letter(cx: &mut TestAppContext) {
    let (overlay, vcx) = overlay(cx);
    overlay.update(vcx, |o, cx| {
        o.model.copy_to(3, false, "/home/u");
        cx.notify();
    });
    vcx.run_until_parked();
    let label = vcx
        .debug_bounds("op1-field-label-text")
        .expect("label drawn");
    let text = "DESTINATION FOLDER";
    let plain = vcx.update(|window, _| {
        let style = window.text_style();
        let run = gpui::TextRun {
            len: text.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(text.into(), gpui::px(12.), &[run], None)
            .width
    });
    let want = f32::from(plain) + 1.2 * text.chars().count() as f32;
    assert!(
        (f32::from(label.size.width) - want).abs() <= 1.,
        "label {} want {want}",
        f32::from(label.size.width)
    );
    assert_eq!(f32::from(label.size.height), 14.);
}

// r1-10
#[test]
fn a_read_only_place_refuses_delete() {
    assert_eq!(trash::delete_route(true, false), trash::Route::Refuse);
}

// r1-11
#[test]
fn space_in_the_copy_to_field_is_the_fields_and_enter_submits() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.copy_to(4, false, "/home/u/work");
    ops.key(&cat, "space", false);
    assert!(ops.open() && ops.outbox.is_empty(), "{:?}", ops.outbox);
    ops.key(&cat, "kp_enter", false);
    assert_eq!(ops.outbox.len(), 1);
}
