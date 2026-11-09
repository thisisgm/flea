// Which presented scenes credit a search generation, and what searchStamps reports of them.
use super::presented::{Presented, Scene, Shown};
use flea::ui_logic_file_search::{Session, Stamps};

fn scene(gen: u64, settled: bool, name: &str) -> Scene {
    Scene {
        gen,
        settled,
        row: 0,
        name: name.into(),
    }
}

fn shown(frame: u64, at_ns: u64) -> Shown {
    Shown {
        frame,
        at_ns: Some(at_ns),
        clock: Some(1),
    }
}

fn session(gen: u64, settled_at: u64) -> Session {
    Session {
        gen,
        query: "ab".into(),
        scanned: 9,
        total: 2,
        answered: gen,
        stamps: Stamps {
            gen,
            key_at: 100,
            first_rows_at: 110,
            settled_at,
        },
        ..Session::default()
    }
}

#[test]
fn the_earliest_presented_scene_of_the_generation_is_its_first() {
    let mut record = Presented::default();
    record.record(1, &scene(1, false, "a/b.txt"), shown(6, 600));
    record.record(1, &scene(1, false, "a/b.txt"), shown(5, 500));
    record.record(1, &scene(1, false, "a/c.txt"), shown(7, 700));
    let json = record.stamps_json(&session(1, 0));
    assert!(
        json.contains(
            r#""firstRow":0,"firstName":"a/b.txt","presentedFirst":{"frame":5,"ns":500,"clock":1}"#
        ),
        "{json}"
    );
    assert!(
        json.ends_with(r#""presentedSettled":null,"coverage":"complete"}"#),
        "an unsettled answer has no settled frame: {json}"
    );
}

#[test]
fn only_a_scene_painted_after_the_final_answer_settles() {
    let mut record = Presented::default();
    record.record(1, &scene(1, false, "x"), shown(3, 300));
    record.record(1, &scene(1, true, "x"), shown(4, 400));
    let json = record.stamps_json(&session(1, 120));
    assert!(json.contains(r#""presentedFirst":{"frame":3,"ns":300,"clock":1},"presentedSettled":{"frame":4,"ns":400,"clock":1}"#), "{json}");
}

#[test]
fn an_older_generation_never_credits_the_newest() {
    let mut record = Presented::default();
    record.record(1, &scene(1, true, "old"), shown(2, 200));
    record.record(2, &scene(1, true, "old"), shown(3, 300));
    let json = record.stamps_json(&session(2, 0));
    assert!(
        json.contains(
            r#""firstRow":null,"firstName":null,"presentedFirst":null,"presentedSettled":null"#
        ),
        "{json}"
    );
    record.record(2, &scene(2, true, "new"), shown(4, 400));
    assert!(record
        .stamps_json(&session(2, 130))
        .contains(r#""firstName":"new","presentedFirst":{"frame":4"#));
    assert!(
        !record.stamps_json(&session(3, 0)).contains("new"),
        "a newer query hides the record until it is presented"
    );
}

#[test]
fn the_answer_fields_and_a_missing_clock_are_reported_as_they_are() {
    let mut record = Presented::default();
    record.record(
        1,
        &scene(1, true, "q \"x\""),
        Shown {
            frame: 9,
            at_ns: None,
            clock: None,
        },
    );
    let mut answered = session(1, 120);
    answered.partial = Some("unreadable".into());
    assert_eq!(
        record.stamps_json(&answered),
        concat!(
            r#"{"gen":1,"keyAt":100,"firstRowsAt":110,"settledAt":120,"query":"ab","indexed":9,"total":2,"#,
            r#""walking":false,"cancelled":false,"partial":"unreadable","firstRow":0,"firstName":"q \"x\"","#,
            r#""presentedFirst":{"frame":9,"ns":null,"clock":null},"presentedSettled":{"frame":9,"ns":null,"clock":null},"#,
            r#""coverage":"complete"}"#
        )
    );
}

#[test]
fn an_unanswered_generation_reports_no_answer_of_an_older_one() {
    let mut unanswered = session(2, 0);
    unanswered.answered = 1;
    unanswered.partial = Some("unreadable".into());
    let json = Presented::default().stamps_json(&unanswered);
    assert!(
        json.contains(
            r#""indexed":null,"total":null,"walking":null,"cancelled":null,"partial":null"#
        ),
        "{json}"
    );
    assert!(Presented::default()
        .stamps_json(&session(2, 9))
        .contains(r#""indexed":9,"total":2,"walking":false,"cancelled":false,"partial":false"#));
}

#[test]
fn feedback_lost_after_the_generation_began_marks_its_coverage_lost() {
    let mut record = Presented::default();
    record.coverage(1, 4, None, true);
    record.record(1, &scene(1, true, "x"), shown(5, 500));
    assert!(record
        .stamps_json(&session(1, 120))
        .ends_with(r#""coverage":"complete"}"#));
    record.coverage(1, 5, None, true);
    assert!(
        record
            .stamps_json(&session(1, 120))
            .ends_with(r#""coverage":"lost"}"#),
        "an expired submission may have been the first"
    );
    record.coverage(2, 5, None, true);
    assert!(
        record
            .stamps_json(&session(2, 0))
            .ends_with(r#""coverage":"complete"}"#),
        "a new generation starts from the current count"
    );
    assert!(
        Presented::default()
            .stamps_json(&session(1, 0))
            .ends_with(r#""coverage":"none"}"#),
        "no feedback means no claim"
    );
}

#[test]
fn an_earlier_submission_still_in_flight_leaves_the_first_pending() {
    let mut record = Presented::default();
    record.coverage(1, 0, Some(4), true);
    record.record(1, &scene(1, true, "x"), shown(5, 500));
    let json = record.stamps_json(&session(1, 120));
    assert!(
        json.ends_with(r#""coverage":"pending"}"#),
        "frame 4 may still be the first shown: {json}"
    );
    record.record(1, &scene(1, true, "x"), shown(4, 400));
    record.coverage(1, 0, None, true);
    let json = record.stamps_json(&session(1, 120));
    assert!(
        json.contains(r#""presentedFirst":{"frame":4,"ns":400,"clock":1}"#),
        "{json}"
    );
    assert!(json.ends_with(r#""coverage":"complete"}"#), "{json}");
}

#[test]
fn coverage_is_claimed_only_while_feedback_is_observed() {
    let mut never = Presented::default();
    never.coverage(1, 0, None, false);
    assert!(
        never
            .stamps_json(&session(1, 0))
            .ends_with(r#""coverage":"none"}"#),
        "a timer without an observer claims nothing"
    );
    let mut closed = Presented::default();
    closed.coverage(1, 0, None, true);
    closed.record(1, &scene(1, true, "x"), shown(3, 300));
    closed.coverage(1, 0, None, false);
    assert!(
        closed
            .stamps_json(&session(1, 120))
            .ends_with(r#""coverage":"lost"}"#),
        "the window closed while observed"
    );
}
