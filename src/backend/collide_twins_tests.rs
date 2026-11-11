// Two sources that land on one name under the destination's rules are asked about like a name dest already holds.
use super::{owned, text, transfer};
use crate::backend::collide::*;
use crate::backend::fatnames::{casefold, test_stand_fat, Choice};
use crate::backend::testdir::TestDir;
use std::path::Path;

fn answered(word: &str, question: Question, dest: &Path, fat: Option<Choice>) -> Policy {
    let id = question.id;
    Ask {
        word: Some(word.to_string()),
        id,
        inner: None,
        fat,
    }
    .policy(Some(question), dest)
}

#[test]
fn case_twins_are_asked_about_where_the_destination_folds_case() {
    let d = TestDir::new("collide-case-twins");
    let to = d.dir("to");
    d.dir("from");
    let (lower, upper) = (d.file("from/a.txt", "lower"), d.file("from/A.txt", "UPPER"));
    let paths = owned(&[&lower, &upper]);
    let (question, shown) = ask(81, &paths, &to.to_string_lossy(), None);
    assert!(
        question.twins.is_empty() && shown.is_empty(),
        "a case-sensitive folder holds both"
    );
    casefold::test_stand_folding(&to, true);
    let (question, shown) = ask(81, &paths, &to.to_string_lossy(), None);
    casefold::test_stand_folding(&to, false);
    assert_eq!(
        question.twins.iter().collect::<Vec<_>>(),
        vec![&upper],
        "the second spelling lands on the first"
    );
    assert!(question.seen.is_empty());
    assert_eq!(
        shown.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        vec!["A.txt"]
    );
}

// "trail." and "trail " both become "trail" under Rename, the S8-5 FAT pair.
fn fat_pair(d: &TestDir) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let to = d.dir("to");
    d.dir("from");
    (
        to,
        d.file("from/trail.", "dot"),
        d.file("from/trail ", "space"),
    )
}

#[test]
fn keep_both_lands_two_sources_that_rename_onto_one_fat_name() {
    let d = TestDir::new("collide-fat-twins-keep");
    let (to, dot, space) = fat_pair(&d);
    test_stand_fat(&to, true);
    let (question, shown) = ask(
        82,
        &owned(&[&dot, &space]),
        &to.to_string_lossy(),
        Some(Choice::Rename),
    );
    assert_eq!(question.twins.len(), 1);
    assert_eq!(
        shown.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        vec!["trail"]
    );
    let policy = answered("keep", question, &to, Some(Choice::Rename));
    let (ok, failed, skipped, errors, _) = transfer(false, &[&dot, &space], &to, policy);
    test_stand_fat(&to, false);
    assert_eq!((ok, failed, skipped), (2, 0, 0), "{errors:?}");
    assert_eq!(text(&to.join("trail")), "dot");
    assert_eq!(
        text(&to.join("trail copy")),
        "space",
        "the twin keeps both under the Duplicate word"
    );
}

#[test]
fn skip_leaves_the_second_twin_and_no_answer_still_refuses_it() {
    let d = TestDir::new("collide-fat-twins-skip");
    let (to, dot, space) = fat_pair(&d);
    test_stand_fat(&to, true);
    let (question, _) = ask(
        83,
        &owned(&[&dot, &space]),
        &to.to_string_lossy(),
        Some(Choice::Rename),
    );
    let policy = answered("skip", question, &to, Some(Choice::Rename));
    assert!(
        policy.covers(&space, &to.join("trail")),
        "Skip leaves the twin before the first one lands, as the sweep counted it"
    );
    let (ok, failed, skipped, _, entry) = transfer(false, &[&dot, &space], &to, policy);
    assert_eq!((ok, failed, skipped), (1, 0, 1));
    assert_eq!(entry.steps.len(), 1);
    std::fs::remove_file(to.join("trail")).unwrap();
    let unanswered = Ask {
        word: None,
        id: 0,
        inner: None,
        fat: Some(Choice::Rename),
    }
    .policy(None, &to);
    let (ok, failed, _, errors, _) = transfer(false, &[&dot, &space], &to, unanswered);
    test_stand_fat(&to, false);
    assert_eq!(
        (ok, failed),
        (1, 1),
        "without an answer the twin is refused, never written over the first"
    );
    assert_eq!(errors, vec!["already exists".to_string()]);
    assert_eq!(text(&to.join("trail")), "dot");
}
