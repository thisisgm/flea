// Ported from PickerWindow.accept, replace, canAccept and doubleActivate and tests/js/picker.js's double-click cases.
use super::*;

fn state<'a>(req: &'a Request, location: &'a str) -> AcceptState<'a> {
    AcceptState {
        req,
        backend_unavailable: false,
        accept_marks: false,
        mark_request: 0,
        review_request: 0,
        marks: 0,
        location,
        listing_loading: false,
        cursor_file: None,
        save_name: "",
        review: None,
        review_asked: false,
    }
}

#[test]
fn a_busy_window_refuses_every_answer() {
    let req = request("{}");
    let base = AcceptState {
        cursor_file: Some("a.txt"),
        ..state(&req, "/d")
    };
    for busy in [
        AcceptState {
            backend_unavailable: true,
            ..base
        },
        AcceptState {
            accept_marks: true,
            ..base
        },
        AcceptState {
            mark_request: 2,
            ..base
        },
        AcceptState {
            review_request: 1,
            ..base
        },
    ] {
        assert_eq!(accept_step(&busy), AcceptStep::Busy);
        assert!(!can_accept(&busy));
    }
    assert_eq!(
        accept_step(&base),
        AcceptStep::MarkCursor("/d/a.txt".into())
    );
    assert!(can_accept(&base));
}

#[test]
fn an_open_takes_the_marks_then_the_cursor_file() {
    let req = request(r#"{"multiple":true}"#);
    let base = state(&req, "/d");
    assert_eq!(
        accept_step(&AcceptState { marks: 2, ..base }),
        AcceptStep::ValidateMarks
    );
    assert_eq!(
        accept_step(&base),
        AcceptStep::Nothing,
        "a folder under the cursor is no answer"
    );
    assert!(!can_accept(&base));
    assert_eq!(
        accept_step(&AcceptState {
            cursor_file: Some("x"),
            ..state(&req, RECENT)
        }),
        AcceptStep::MarkCursor("/x".into())
    );
}

#[test]
fn a_folder_request_with_nothing_checked_takes_where_it_stands() {
    for text in [r#"{"directory":true}"#, r#"{"mode":"savefiles"}"#] {
        let req = request(text);
        assert_eq!(
            accept_step(&state(&req, "/srv")),
            AcceptStep::MarkFolder("/srv".into()),
            "{text}"
        );
        assert!(can_accept(&state(&req, "/srv")));
        assert!(
            !can_accept(&AcceptState {
                listing_loading: true,
                ..state(&req, "/srv")
            }),
            "not while it loads"
        );
        assert_eq!(
            accept_step(&state(&req, RECENT)),
            AcceptStep::RecentFolder,
            "Recent is not a directory"
        );
        assert!(!can_accept(&state(&req, RECENT)));
        assert!(can_accept(&AcceptState {
            marks: 1,
            ..state(&req, RECENT)
        }));
    }
}

#[test]
fn a_save_asks_through_its_name_the_probe_and_the_review() {
    let req = request(r#"{"mode":"save"}"#);
    let free = SaveReview {
        folder: "/d".into(),
        name: "a.txt".into(),
        collision: false,
        review: 7,
        path: "/d/a.txt".into(),
    };
    let taken = SaveReview {
        collision: true,
        ..free.clone()
    };
    let stale = SaveReview {
        name: "b.txt".into(),
        ..free.clone()
    };
    let named = AcceptState {
        save_name: "a.txt",
        ..state(&req, "/d")
    };
    assert_eq!(accept_step(&state(&req, "/d")), AcceptStep::SaveNameEmpty);
    assert_eq!(
        accept_step(&AcceptState {
            save_name: "../x",
            ..named
        }),
        AcceptStep::SaveNameRefused
    );
    assert_eq!(accept_step(&named), AcceptStep::ProbeSave);
    assert_eq!(
        accept_step(&AcceptState {
            review: Some(&stale),
            ..named
        }),
        AcceptStep::ProbeSave,
        "a review of another name is stale"
    );
    assert!(!can_accept(&AcceptState {
        review: Some(&stale),
        ..named
    }));
    assert_eq!(
        accept_step(&AcceptState {
            review: Some(&free),
            ..named
        }),
        AcceptStep::SubmitSave
    );
    assert!(can_accept(&AcceptState {
        review: Some(&free),
        ..named
    }));
    let asking = AcceptState {
        review: Some(&taken),
        ..named
    };
    assert_eq!(accept_step(&asking), AcceptStep::AskReview);
    assert!(
        !replace_allowed(&asking),
        "Replace waits for the asked review"
    );
    let reviewing = AcceptState {
        review_asked: true,
        ..asking
    };
    assert_eq!(
        accept_step(&reviewing),
        AcceptStep::Reviewing,
        "Return never overwrites"
    );
    assert!(replace_allowed(&reviewing));
    assert!(
        !replace_allowed(&AcceptState {
            review_request: 3,
            ..reviewing
        }),
        "a submitted review is not resubmitted"
    );
    assert!(
        !replace_allowed(&AcceptState {
            review: Some(&free),
            ..reviewing
        }),
        "a free name has nothing to replace"
    );
}

#[test]
fn reviewed_marks_keep_order_and_their_first_uri() {
    let previous = [Mark {
        path: "/x/a #.png".into(),
        bytes: 10,
        uri: "file:///x/a%20%23.png".into(),
    }];
    let marks = Mark::reviewed(
        &previous,
        &[("/x/a #.png".into(), 15), ("/x/b.png".into(), 20)],
    );
    assert_eq!(
        marks
            .iter()
            .map(|mark| mark.path.as_str())
            .collect::<Vec<_>>(),
        ["/x/a #.png", "/x/b.png"]
    );
    assert_eq!(
        (marks[0].bytes, marks[0].uri.as_str()),
        (15, "file:///x/a%20%23.png")
    );
    assert_eq!(marks[1].uri, "file:///x/b.png");
    let uris: Vec<String> = marks.iter().map(|mark| mark.uri.clone()).collect();
    assert_eq!(
        reply(RESPONSE_OK, &uris, Some(1)),
        r#"{"response":0,"uris":["file:///x/a%20%23.png","file:///x/b.png"],"filter":1}"#
    );
    assert!(
        Mark::reviewed(&previous, &[]).is_empty(),
        "removed identities leave the result"
    );
}

#[test]
fn a_double_click_marks_sends_or_opens_by_request() {
    let single = request("{}");
    let multi = request(r#"{"multiple":true}"#);
    let file = Some(false);
    let folder = Some(true);
    assert_eq!(
        double_action(&single, file, "/a/b.txt", "/a/b.txt", false),
        DoubleAction::MarkAccept
    );
    assert_eq!(
        double_action(&single, file, "/a/b.txt", "/a/b.txt", true),
        DoubleAction::Accept
    );
    assert_eq!(
        double_action(&multi, file, "/a/c.txt", "/a/c.txt", false),
        DoubleAction::Mark
    );
    assert_eq!(
        double_action(&multi, file, "/a/b.txt", "/a/b.txt", true),
        DoubleAction::Mark
    );
    assert_eq!(
        double_action(&single, folder, "/a/sub", "/a/sub", false),
        DoubleAction::Open
    );
    assert_eq!(
        double_action(&multi, folder, "/a/sub", "/a/sub", false),
        DoubleAction::Open
    );
    assert_eq!(
        double_action(&multi, file, "/a/b.txt", "/a/c.txt", false),
        DoubleAction::None
    );
    for refusing in [
        r#"{"directory":true}"#,
        r#"{"mode":"save"}"#,
        r#"{"mode":"savefiles","multiple":true}"#,
    ] {
        assert_eq!(
            double_action(&request(refusing), file, "/a/b.txt", "/a/b.txt", false),
            DoubleAction::None,
            "{refusing}"
        );
    }
    assert_eq!(
        double_action(&single, file, "/a/b.txt", "/a/c.txt", false),
        DoubleAction::None
    );
    assert_eq!(
        double_action(&single, file, "/a/b.txt", "", false),
        DoubleAction::None
    );
    assert_eq!(
        double_action(&single, None, "/a/b.txt", "/a/b.txt", false),
        DoubleAction::None
    );
    assert!(same_tap("/a/b.txt", "/a/b.txt") && !same_tap("/a/b.txt", "/a/c.txt"));
    assert!(!same_tap("", "/a/b.txt") && !same_tap("/a/b.txt", ""));
}
