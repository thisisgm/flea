use super::*;

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
