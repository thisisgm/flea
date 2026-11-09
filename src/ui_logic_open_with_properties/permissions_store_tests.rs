// tests/js/permissions-skips.js: the several-item store and its note.
use super::english;
use super::permissions::*;

fn modes(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_store_answers_done_once_on_the_last_of_five_thousand_replies() {
    const REPLIES: usize = 5000;
    let mut store = Store::new(REPLIES);
    let mut completions = 0;
    for at in 0..REPLIES {
        let done = store.note(
            at,
            &format!("/f{at}"),
            Inspected::Ok {
                mode: "0644".into(),
                reason: String::new(),
            },
        );
        assert_eq!(
            done,
            at + 1 == REPLIES,
            "done answers only on the last reply"
        );
        completions += usize::from(done);
    }
    assert_eq!(completions, 1);
    assert!(store.modes.iter().all(|m| m == "0644"));
    assert!(
        !store.note(
            REPLIES,
            "/past",
            Inspected::Ok {
                mode: "0644".into(),
                reason: String::new()
            }
        ),
        "a stray index lands nowhere"
    );
}

#[test]
fn a_refused_inspect_lands_as_a_named_skip_beside_its_row() {
    let mut store = Store::new(3);
    store.note(
        0,
        "/d/a.txt",
        Inspected::Ok {
            mode: "2755".into(),
            reason: "Read-only: setgid bit is present.".into(),
        },
    );
    store.note(
        1,
        "/d/b.txt",
        Inspected::Refused {
            error: "Gone.".into(),
        },
    );
    let last = store.note(
        2,
        "/d/c.txt",
        Inspected::Ok {
            mode: "0644".into(),
            reason: String::new(),
        },
    );
    assert_eq!(
        format!("{last}|{}|{}", store.skipped.len(), store.skipped[0].why),
        "true|1|Gone."
    );
    assert_eq!(
        store.reasons.join("|"),
        "Read-only: setgid bit is present.|Gone.|"
    );
    let mut blank = Store::new(1);
    blank.note(
        0,
        "/d/x",
        Inspected::Refused {
            error: String::new(),
        },
    );
    assert_eq!(
        blank.reasons[0], FAILED,
        "a refusal without words takes the card's own"
    );
}

#[test]
fn one_note_names_reasoned_and_refused_rows_together() {
    let i18n = english();
    let paths = modes(&["/d/a.txt", "/d/b.txt", "/d/c.txt"]);
    let store = Store {
        modes: modes(&["0644", "0644", ""]),
        reasons: modes(&["", NOT_OWNER, "Gone."]),
        skipped: Vec::new(),
        pending: 0,
    };
    assert_eq!(
        store.note_text(&i18n, &paths),
        "2 items keep their modes because they cannot be changed: b.txt, c.txt."
    );
    let clean = Store {
        modes: modes(&["0644"]),
        reasons: modes(&[""]),
        skipped: Vec::new(),
        pending: 0,
    };
    assert_eq!(clean.note_text(&i18n, &modes(&["/d/a.txt"])), "");
}
