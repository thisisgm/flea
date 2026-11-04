// Ported from PickerWindow.finish, PickerLifecycle and tests/picker-stall.qml: one answer, then both children, then exit.
use super::*;

fn answered_ok(lifetime: &mut Lifetime) -> Vec<Effect> {
    lifetime.finish(RESPONSE_OK, &[file_uri("/a/b c.txt")], Some(0))
}

#[test]
fn exactly_one_answer_leaves_whichever_way_it_is_asked() {
    let mut lifetime = Lifetime::default();
    assert_eq!(
        answered_ok(&mut lifetime),
        [
            Effect::CloseChecks,
            Effect::WriteReply(
                r#"{"response":0,"uris":["file:///a/b%20c.txt"],"filter":0}"#.into()
            )
        ]
    );
    assert!(lifetime.answered());
    assert!(
        lifetime.cancel().is_empty(),
        "a later Cancel answers nothing"
    );
    assert!(
        lifetime.window_closed().is_empty(),
        "a later close answers nothing"
    );
    assert!(
        answered_ok(&mut lifetime).is_empty(),
        "a second accept answers nothing"
    );
}

#[test]
fn cancel_and_a_window_manager_close_refuse_with_no_uri() {
    let mut cancelled = Lifetime::default();
    assert_eq!(
        cancelled.cancel(),
        [
            Effect::CloseChecks,
            Effect::WriteReply(r#"{"response":1}"#.into())
        ]
    );
    let mut closed = Lifetime::default();
    assert_eq!(
        closed.window_closed(),
        [
            Effect::CloseChecks,
            Effect::WriteReply(r#"{"response":1}"#.into())
        ]
    );
}

#[test]
fn a_lost_backend_is_not_told_to_close_but_the_reply_is_still_written() {
    let mut lifetime = Lifetime::default();
    assert!(lifetime.backend_failed(), "the first loss is news");
    assert!(!lifetime.backend_failed(), "a second report is not");
    assert!(lifetime.backend_unavailable());
    assert_eq!(
        lifetime.cancel(),
        [Effect::WriteReply(r#"{"response":1}"#.into())]
    );
}

#[test]
fn the_children_quit_only_after_the_reply_write_completes() {
    let mut lifetime = Lifetime::default();
    assert!(
        lifetime.reply_written().is_empty(),
        "no answer, nothing written, nothing to quit"
    );
    answered_ok(&mut lifetime);
    assert_eq!(lifetime.reply_written(), [Effect::QuitChildren]);
    assert!(
        lifetime.reply_written().is_empty(),
        "a repeated completion quits once"
    );
}

#[test]
fn the_window_exits_once_after_both_children_stop_in_either_order() {
    for order in [
        [Child::Checks, Child::Listing],
        [Child::Listing, Child::Checks],
    ] {
        let mut lifetime = Lifetime::default();
        answered_ok(&mut lifetime);
        lifetime.reply_written();
        assert!(
            lifetime.child_stopped(order[0]).is_empty(),
            "one stalled child holds the exit"
        );
        assert!(!lifetime.exited());
        assert!(
            lifetime.child_stopped(order[0]).is_empty(),
            "the same child twice is still one"
        );
        assert_eq!(lifetime.child_stopped(order[1]), [Effect::Exit]);
        assert!(
            lifetime.child_stopped(order[1]).is_empty(),
            "the exit fires once"
        );
    }
}

#[test]
fn a_child_that_never_started_still_counts_as_stopped() {
    // picker-stall missing-order: the failed start arrives while the quit is already under way.
    let mut lifetime = Lifetime::default();
    assert!(lifetime.child_stopped(Child::Listing).is_empty());
    lifetime.cancel();
    lifetime.reply_written();
    assert_eq!(lifetime.child_stopped(Child::Checks), [Effect::Exit]);
}

#[test]
fn a_check_line_names_its_op_and_its_id() {
    let mark = Check::Mark {
        path: "/a/\"q\".txt",
        directory: false,
        multiple: true,
    };
    assert_eq!(
        check_line(3, &mark),
        r#"{"op":"mark","path":"/a/\"q\".txt","directory":false,"multiple":true,"c":"picker","id":3}"#
    );
    assert_eq!(
        check_line(
            4,
            &Check::Save {
                folder: "/a",
                name: "b.txt"
            }
        ),
        r#"{"op":"save","folder":"/a","name":"b.txt","c":"picker","id":4}"#
    );
    assert_eq!(
        check_line(5, &Check::Review { review: 9 }),
        r#"{"op":"review","review":9,"c":"picker","id":5}"#
    );
    assert_eq!(
        check_line(6, &Check::Validate),
        r#"{"op":"validate","c":"picker","id":6}"#
    );
    assert_eq!(CLOSE_LINE, r#"{"c":"picker","op":"close"}"#);
}

#[test]
fn a_check_result_is_read_and_a_malformed_one_is_dropped() {
    let mark = check_result(r#"{"t":"picker","id":3,"op":"mark","ok":true,"marks":[{"path":"/a b","bytes":2}],"removed":1}"#).unwrap();
    assert_eq!(
        (mark.id, mark.op.as_str(), mark.ok, mark.removed),
        (3, "mark", true, 1)
    );
    assert_eq!(mark.marks, [("/a b".to_string(), 2)]);
    let refused = check_result(r#"{"t":"picker","id":4,"op":"review","ok":false,"error":"The save location changed; review it again."}"#).unwrap();
    assert!(!refused.ok && refused.error.starts_with("The save location"));
    let save = check_result(
        r#"{"t":"picker","id":5,"op":"save","ok":true,"review":5,"path":"/d/x","collision":true}"#,
    )
    .unwrap();
    assert_eq!(
        (save.review, save.path.as_str(), save.collision),
        (5, "/d/x", true)
    );
    let skipped = check_result(
        r#"{"t":"picker","id":6,"op":"select","ok":true,"marks":[],"removed":0,"skipped":[{"path":"/x","why":"gone"}]}"#,
    );
    assert_eq!(skipped.unwrap().skipped, 1);
    for malformed in [
        "",
        "not json",
        r#"{"t":"rows","id":3}"#,
        r#"{"t":"picker","op":"mark","ok":true}"#,
        r#"{"t":"picker","id":0,"ok":true}"#,
        r#"{"t":"picker","id":-1,"ok":true}"#,
        r#"{"t":"picker","id":2,"ok":true,"marks":{}}"#,
        r#"{"t":"picker","id":2,"ok":true,"marks":[{"path":"/a"}]}"#,
        r#"{"t":"picker","id":2,"ok":true,"marks":[{"path":"/a","bytes":-1}]}"#,
    ] {
        assert_eq!(check_result(malformed), None, "{malformed}");
    }
    assert!(
        !check_result(r#"{"t":"picker","id":7,"ok":"true"}"#)
            .unwrap()
            .ok,
        "only a boolean true is ok"
    );
}

// Review r1-1 (reviewer's repro): picker-stall's negative, lost children never exit an unanswered window.
#[test]
fn rev_pk1_r1_children_lost_before_an_answer_never_exit_the_window() {
    let mut lifetime = Lifetime::default();
    assert!(lifetime.backend_failed());
    let early = [
        lifetime.child_stopped(Child::Checks),
        lifetime.child_stopped(Child::Listing),
    ]
    .concat();
    assert!(
        early.is_empty(),
        "PickerLifecycle exits only after the reply is saved, got {early:?}"
    );
    assert!(!lifetime.answered());
}

// Both children already reaped: the saved reply is the last event, so it carries the exit itself.
#[test]
fn a_reply_saved_after_both_children_stopped_exits_once() {
    for order in [
        [Child::Checks, Child::Listing],
        [Child::Listing, Child::Checks],
    ] {
        let mut lifetime = Lifetime::default();
        lifetime.backend_failed();
        assert!(lifetime.child_stopped(order[0]).is_empty());
        assert!(lifetime.child_stopped(order[1]).is_empty());
        assert_eq!(
            lifetime.cancel(),
            [Effect::WriteReply(r#"{"response":1}"#.into())]
        );
        assert_eq!(
            lifetime.reply_written(),
            [Effect::QuitChildren, Effect::Exit]
        );
        assert!(lifetime.exited());
        assert!(
            lifetime.reply_written().is_empty() && lifetime.child_stopped(order[1]).is_empty(),
            "the exit fires once"
        );
    }
}
