// Review r6's finding as a test: with both held slots full, a refused held Trash delete must still request a redraw.
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

// r6-1: both slots full, then the held Trash delete is refused. The error is said and a redraw is requested.
#[test]
fn a_refused_held_trash_delete_with_both_slots_full_requests_a_redraw() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), 0);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    ops.confirm_delete(review("9"), trash::Scope::Items, Source::Menu(9), 100);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    ops.copy_to(7, false, "/x");
    assert!(ops.trash_held.is_some() && ops.menu_held.is_some());
    ops.line(
        &cat,
        r#"{"t":"trashbrowse","id":1,"op":"delete","ok":false,"stale":false,"error":"boom"}"#,
        300,
    );
    let said = ops.said.last().expect("the refusal is said");
    assert_eq!((said.text.as_str(), said.error), ("boom", true));
    assert!(
        ops.changed,
        "the said error must draw now, not on the next event"
    );
}

// S8-8: a finished transfer says an error unless its one failure was already reported, then a receipt marked to wait behind its errors; a clean one says a plain notice.
#[test]
fn a_failed_transfers_receipt_is_marked_behind_and_a_clean_ones_is_not() {
    let cat = crate::test_catalogs::english();
    let (error, behind, notice) = ((true, false), (false, true), (false, false));
    for (reported, ok, failed, expected) in [
        (true, 0, 1, &[behind][..]),
        (false, 0, 1, &[error, behind][..]),
        (false, 0, 2, &[error, behind][..]),
        (true, 0, 2, &[error, behind][..]),
        (false, 2, 0, &[notice][..]),
    ] {
        let mut ops = Operations::new("default");
        ops.line(
            &cat,
            r#"{"t":"transferstarted","id":13,"n":2,"moving":false}"#,
            0,
        );
        if reported {
            ops.line(
                &cat,
                r#"{"t":"transferitem","id":13,"index":0,"name":"a.txt","ok":false,"err":"permission denied"}"#,
                50,
            );
        }
        let done = format!(r#"{{"t":"transferdone","id":13,"ok":{ok},"failed":{failed}}}"#);
        let before = ops.said.len();
        ops.line(&cat, &done, 100);
        let said: Vec<(bool, bool)> = ops.said[before..]
            .iter()
            .map(|said| (said.error, said.behind))
            .collect();
        assert_eq!(
            said, expected,
            "reported {reported}, ok {ok}, failed {failed}"
        );
    }
}
