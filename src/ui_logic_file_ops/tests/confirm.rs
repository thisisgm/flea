// The permanent card's wire against ui/TrashView.qml and ui/MenuActionDialog.qml: who it answers, its check, its refusals.
use crate::ui_logic_file_ops::collide::{texts, Request};
use crate::ui_logic_file_ops::confirm::{Answer, Confirm, Source, CHECK_MS};
use crate::ui_logic_file_ops::testcat::Catalog;
use crate::ui_logic_file_ops::trash::{Review, Scope};
use crate::ui_logic_file_ops::{parse, Event, Reply};

fn review(token: &str) -> Review {
    Review {
        count: Some(3),
        token: token.into(),
        ..Default::default()
    }
}

fn trash() -> Confirm {
    Confirm::new(
        Source::Trash(Request::new().with("uris", &texts(&["trash:///a"]))),
        Scope::TrashItems,
        review("4"),
        0,
    )
}

fn reply(line: &str) -> Reply {
    match parse(line) {
        Some(Event::TrashBrowse(r) | Event::MenuAction(r)) => r,
        other => panic!("not a reply: {other:?}"),
    }
}

#[test]
fn a_reply_parses_with_its_raw_token() {
    // src/backend/trashbrowse.rs:705
    let r = reply(
        r#"{"t":"trashbrowse","id":3,"op":"prepare","ok":true,"token":9,"all":true,"count":2,"bytes":10,"partial":false}"#,
    );
    assert_eq!(
        (
            r.id,
            r.op.as_str(),
            r.ok,
            r.token.as_str(),
            r.all,
            r.count,
            r.bytes
        ),
        (3, "prepare", true, "9", true, 2, 10)
    );
    assert!(parse(r#"{"t":"trashbrowse","op":"delete"}"#).is_none());
}

#[test]
fn only_the_reply_to_what_went_out_is_taken() {
    let cat = Catalog::english();
    let mut c = trash();
    let mut next = 0;
    assert_eq!(
        c.delete(&mut next, false).as_deref(),
        Some(r#"{"c":"trashbrowse","op":"delete","id":1,"token":4}"#)
    );
    assert_eq!(c.deleting(), Some(1));
    // A reply to another id, another op or the other session changes nothing.
    for line in [
        r#"{"t":"trashbrowse","id":2,"op":"delete","ok":true}"#,
        r#"{"t":"trashbrowse","id":1,"op":"check","ok":true,"valid":false}"#,
    ] {
        assert_eq!(
            c.reply(&cat.i18n, true, &reply(line), 0, &mut next),
            Answer::Ignored
        );
    }
    assert_eq!(
        c.reply(
            &cat.i18n,
            false,
            &reply(r#"{"t":"menuaction","id":1,"op":"delete","ok":true}"#),
            0,
            &mut next
        ),
        Answer::Ignored
    );
    assert_eq!(
        c.reply(
            &cat.i18n,
            true,
            &reply(r#"{"t":"trashbrowse","id":1,"op":"delete","ok":true,"done":3,"failed":0}"#),
            0,
            &mut next
        ),
        Answer::Deleted {
            close: None,
            said: None
        }
    );
}

#[test]
fn a_hidden_or_busy_card_sends_no_second_delete_and_no_check() {
    let mut c = trash();
    let mut next = 0;
    assert!(c.delete(&mut next, false).is_some());
    assert!(c.delete(&mut next, false).is_none());
    assert_eq!((c.due(), c.tick(CHECK_MS * 4, &mut next)), (None, None));
    // An Empty Trash already running keeps Delete off.
    let mut c = trash();
    assert!(c.delete(&mut next, true).is_none() && c.shown);
}

#[test]
fn a_trash_refusal_falls_back_to_the_trash_sentence() {
    let cat = Catalog::english();
    let mut c = trash();
    let mut next = 0;
    c.delete(&mut next, false);
    let answer = c.reply(
        &cat.i18n,
        true,
        &reply(r#"{"t":"trashbrowse","id":1,"op":"delete","ok":false,"stale":false,"error":""}"#),
        0,
        &mut next,
    );
    // ui/TrashView.qml:206
    assert_eq!(
        answer,
        Answer::Refused {
            error: "That Trash action could not be completed.".into(),
            close: None
        }
    );
}

#[test]
fn a_menu_card_refreshes_when_stale_and_closes_when_refused() {
    let cat = Catalog::english();
    let mut c = Confirm::new(Source::Menu(5), Scope::Items, review("2"), 0);
    let mut next = 0;
    assert_eq!(
        c.cancel(&mut next),
        r#"{"c":"menuaction","op":"close","id":5}"#
    );
    assert_eq!(
        c.delete(&mut next, false).as_deref(),
        Some(r#"{"c":"menuaction","op":"delete","id":5,"token":2}"#)
    );
    // ui/MenuActionDialog.qml:97, a delete the backend could not finish answers ok with stale.
    let stale = c.reply(
        &cat.i18n,
        false,
        &reply(r#"{"t":"menuaction","id":5,"op":"delete","ok":true,"stale":true,"error":"gone"}"#),
        0,
        &mut next,
    );
    assert_eq!(
        stale,
        Answer::Refresh(r#"{"c":"menuaction","op":"refreshDelete","id":5}"#.into())
    );
    assert_eq!(c.reply(&cat.i18n, false, &reply(r#"{"t":"menuaction","id":5,"op":"refreshDelete","ok":true,"token":3,"count":1,"bytes":4}"#), 10, &mut next), Answer::Shown);
    assert_eq!(
        (c.review.token.as_str(), c.review.count, c.shown, c.due()),
        ("3", Some(1), true, Some(10 + CHECK_MS))
    );
    c.tick(10 + CHECK_MS, &mut next);
    // ui/MenuActionDialog.qml:94
    let refused = c.reply(
        &cat.i18n,
        false,
        &reply(r#"{"t":"menuaction","id":5,"op":"checkDelete","ok":false,"error":""}"#),
        0,
        &mut next,
    );
    assert_eq!(
        refused,
        Answer::Refused {
            error: "That action could not be completed.".into(),
            close: Some(r#"{"c":"menuaction","op":"close","id":5}"#.into())
        }
    );
}
