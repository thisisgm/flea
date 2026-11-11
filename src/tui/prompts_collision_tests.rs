use super::*;

#[test]
fn answering_without_a_pending_paste_has_no_effects() {
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    for key in [Key::character('r', ""), Key::named("Escape", "")] {
        answer(&mut m, &key, &mut w).unwrap();
        assert!(m.prompts.question.is_none());
    }
    assert!(drain(&mut w).is_empty());
    finish(w, reader);
}

#[test]
fn a_stale_collision_leaves_the_pending_paste_available() {
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    m.clipboard = vec!["/elsewhere/notes.txt".into()];
    press(&mut m, &mut w, Key::character('p', ""));
    let id = count(requests(&drain(&mut w), "collisions")[0], "id");
    event(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"collisions","id":{},"total":0}}"#, id + 1),
    );
    assert_eq!(m.prompts.asked.as_ref().map(|paste| paste.id), Some(id));
    assert!(drain(&mut w).is_empty());
    event(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"collisions","id":{id},"total":0}}"#),
    );
    assert_eq!(requests(&drain(&mut w), "transfer").len(), 1);
    assert!(m.prompts.asked.is_none());
    finish(w, reader);
}

// tui-collision: the cursor waits on the name the paste meets; a listing left before the answer keeps its cursor.
#[test]
fn a_collision_puts_the_cursor_on_the_name_the_paste_meets() {
    for (moved, expected) in [(false, 7), (true, 0)] {
        let (mut w, reader) = echo_wire();
        let mut m = frame_tests::model();
        m.cursor = 0;
        m.clipboard = vec!["/elsewhere/notes.txt".into()];
        press(&mut m, &mut w, Key::character('p', ""));
        if moved {
            m.path = "/fx/v/work/Docs".into();
        }
        let id = count(requests(&drain(&mut w), "collisions")[0], "id");
        event(
            &mut m,
            &mut w,
            &format!(
                r#"{{"t":"collisions","id":{id},"total":1,"names":[{{"n":"notes.txt","d":false,"i":"text-plain"}}],"mergeable":false}}"#
            ),
        );
        assert!(m.prompts.question.is_some());
        assert_eq!(
            m.cursor, expected,
            "listing left before the answer: {moved}"
        );
        finish(w, reader);
    }
}
// A name past the held rows is located, and the cursor lands on it when the answer comes.
#[test]
fn a_collision_past_the_held_rows_locates_the_name() {
    let (mut w, reader) = echo_wire();
    let mut m = frame_tests::model();
    (m.cursor, m.total) = (0, 5000);
    m.clipboard = vec!["/elsewhere/zz.txt".into()];
    press(&mut m, &mut w, Key::character('p', ""));
    let id = count(requests(&drain(&mut w), "collisions")[0], "id");
    event(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"collisions","id":{id},"total":1,"names":[{{"n":"zz.txt","d":false,"i":"text-plain"}}],"mergeable":false}}"#
        ),
    );
    let sent = drain(&mut w);
    let locate = requests(&sent, "locate");
    assert_eq!(
        locate.len(),
        1,
        "the name off the held rows was not located"
    );
    assert_eq!(text(locate[0], "path"), "/fx/v/work/zz.txt");
    event(
        &mut m,
        &mut w,
        r#"{"t":"located","directory":"/fx/v/work","path":"/fx/v/work/zz.txt","index":4321}"#,
    );
    assert_eq!(m.cursor, 4321);
    assert!(
        m.prompts.question.is_some(),
        "the question stays while the cursor lands"
    );
    finish(w, reader);
}
// NC-1: a name that is not UTF-8 meets its own row and locate path through "x", never its lossy twin's.
#[test]
fn a_raw_collision_name_meets_its_own_row_not_its_lossy_twin() {
    let exact = flea::bytename::name_text(b"col\xff").into_owned();
    let lossy = flea::bytename::display(&exact).into_owned();
    assert_ne!(exact, lossy);
    let line = |id: usize| {
        format!(
            r#"{{"t":"collisions","id":{id},"total":1,"names":[{{"n":"{}","d":false,"i":"text-plain","x":"{}"}}],"mergeable":false}}"#,
            crate::json::escape(&lossy),
            crate::json::escape(&exact)
        )
    };
    for held in [true, false] {
        let (mut w, reader) = echo_wire();
        let mut m = frame_tests::model();
        m.cursor = 0;
        m.rows.get_mut(&2).unwrap().name = lossy.clone();
        if held {
            m.rows.get_mut(&4).unwrap().name = exact.clone();
        } else {
            m.total = 5000;
        }
        m.clipboard = vec![format!("/elsewhere/{exact}")];
        press(&mut m, &mut w, Key::character('p', ""));
        let id = count(requests(&drain(&mut w), "collisions")[0], "id");
        event(&mut m, &mut w, &line(id));
        let sent = drain(&mut w);
        let locate = requests(&sent, "locate");
        if held {
            assert_eq!(m.cursor, 4, "the cursor waits on the raw name's own row");
            assert!(locate.is_empty());
        } else {
            assert_eq!(locate.len(), 1);
            assert_eq!(
                text(locate[0], "path"),
                format!("/fx/v/work/{exact}"),
                "the locate names the raw file"
            );
            assert_eq!(m.cursor, 0, "the lossy twin's row is never taken for it");
        }
        assert_eq!(
            m.prompts.question.as_ref().map(|q| q.name.clone()),
            Some(exact.clone())
        );
        finish(w, reader);
    }
}
