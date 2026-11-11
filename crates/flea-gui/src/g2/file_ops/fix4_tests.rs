// Review r4's findings as tests: a menu refresh keeps its card busy and modal, a replaced Trash review sends nothing, and a Trash delete out outlives a menu card.
use super::model::Operations;
use super::tests::trash_source;
use flea::ui_logic_file_ops::{confirm::Source, trash};

fn review(token: &str) -> trash::Review {
    trash::Review {
        count: Some(2),
        token: token.into(),
        ..Default::default()
    }
}

const MENU_REFRESH: &str =
    r#"{"t":"menuaction","id":6,"op":"checkDelete","ok":true,"valid":false}"#;
const MENU_REFRESHED: &str =
    r#"{"t":"menuaction","id":6,"op":"refreshDelete","ok":true,"token":7,"count":2,"bytes":4}"#;

// r4-1: while refreshDelete is out the card stays drawn and modal; Escape and Cancel close it, Delete is out of reach.
#[test]
fn a_menu_refresh_keeps_a_busy_card_with_cancel_live() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("3"), trash::Scope::Items, Source::Menu(6), 0);
    ops.tick(&cat, 2500);
    ops.line(&cat, MENU_REFRESH, 2600);
    ops.outbox.clear();
    assert!(ops.open(), "the card must stay drawn during refreshDelete");
    assert!(ops.key(&cat, "j", false) && ops.key(&cat, "l", false));
    assert!(ops.outbox.is_empty(), "{:?}", ops.outbox);
    assert!(ops.key(&cat, "enter", false));
    assert_eq!(
        ops.outbox,
        vec![r#"{"c":"menuaction","op":"close","id":6}"#]
    );
    assert!(!ops.open());
}

#[test]
fn escape_closes_a_menu_refresh_card() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("3"), trash::Scope::Items, Source::Menu(6), 0);
    ops.tick(&cat, 2500);
    ops.line(&cat, MENU_REFRESH, 2600);
    ops.outbox.clear();
    assert!(ops.key(&cat, "escape", false));
    assert_eq!(
        ops.outbox,
        vec![r#"{"c":"menuaction","op":"close","id":6}"#]
    );
    assert!(!ops.open());
}

// r4-1: the card comes back when the fresh review lands, and listing keys then act on the card as on any open card.
#[test]
fn the_menu_card_comes_back_with_its_review_and_keys_follow_it() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("3"), trash::Scope::Items, Source::Menu(6), 0);
    ops.tick(&cat, 2500);
    ops.line(&cat, MENU_REFRESH, 2600);
    ops.line(&cat, MENU_REFRESHED, 9000);
    assert!(ops.open());
    ops.outbox.clear();
    ops.key(&cat, "l", false);
    ops.key(&cat, "space", false);
    assert_eq!(
        ops.outbox,
        vec![r#"{"c":"menuaction","op":"delete","id":6,"token":7}"#]
    );
}

// r4-2: a replaced Trash review sends nothing; the Trash view's request already superseded it on the backend.
#[test]
fn a_replaced_trash_review_sends_nothing() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), 0);
    ops.tick(&cat, 2500);
    ops.line(
        &cat,
        r#"{"t":"trashbrowse","id":1,"op":"check","ok":true,"valid":false}"#,
        2600,
    );
    ops.requests += 1;
    ops.outbox.clear();
    ops.line(&cat, r#"{"t":"trashbrowse","id":2,"op":"prepare","ok":false,"stale":false,"error":"Trash request cancelled."}"#, 2700);
    assert!(ops.outbox.is_empty(), "{:?}", ops.outbox);
    assert!(ops.confirm.is_none() && !ops.open());
}

// r4-3: a menu card over a Trash delete still out leaves the delete held: the seam stays true and its start lands.
#[test]
fn a_menu_card_over_a_trash_delete_out_keeps_the_seam_and_its_start() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), 0);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    assert!(ops.trash_request_out());
    ops.confirm_delete(review("9"), trash::Scope::Items, Source::Menu(9), 100);
    assert!(
        ops.trash_request_out(),
        "a held Trash delete must still hold the seam"
    );
    ops.line(
        &cat,
        r#"{"t":"trashdeletestarted","id":40,"requestId":1}"#,
        200,
    );
    assert_eq!((ops.emptying.id, ops.emptying.running), (40, true));
}

// r4-1: Delete is out of reach while the menu's refresh is out, even by press.
#[test]
fn a_busy_menu_card_takes_no_delete_press() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("3"), trash::Scope::Items, Source::Menu(6), 0);
    ops.tick(&cat, 2500);
    ops.line(&cat, MENU_REFRESH, 2600);
    ops.outbox.clear();
    ops.press(&cat, super::press::Press::Delete(trash::Button::Delete));
    assert!(ops.open() && ops.outbox.is_empty(), "{:?}", ops.outbox);
}

// The working note is the catalogue's own sentence, the one v0.3.12 shows.
#[test]
fn the_working_note_reads_as_v0312_has_it() {
    assert_eq!(
        crate::test_catalogs::english().tr("jobs.working"),
        "Working\u{2026}"
    );
}
