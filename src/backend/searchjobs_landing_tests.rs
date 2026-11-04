//! Walk landings that apply queued watch deltas re-answer the standing query on fresh row ids.
use super::*;

#[test]
fn search_landing_watch_deltas_never_restore_renumbered_rest() {
    let root = TestDir::new("search-delta-landing");
    root.dir("scope");
    for i in 0..40 {
        root.file(&format!("scope/alpha{i:02}.txt"), "a");
    }
    let mut lp = Loop::open(&root, "alpha", 2);
    assert_eq!(lp.session().pending_rest_len(), 38);
    let gone = root.join("scope/alpha07.txt");
    root.assert_contains(&gone);
    std::fs::remove_file(&gone).unwrap();
    lp.delta();
    // The landing reconciles the queued deltas itself, so it re-runs the standing query the consumed delta would have.
    lp.session().announced = false;
    lp.route(Event::SearchWalked);
    lp.ready();
    let landed = lp.said();
    assert_eq!(kinds(&landed), ["listed", "rows", "searched"], "{landed:?}");
    assert!(
        landed
            .iter()
            .all(|line| crate::json::field_usize(line, "gen") == Some(1)),
        "the standing gen owns the landing's answer"
    );
    assert_eq!(crate::json::field_usize(&landed[2], "n"), Some(39));
    assert!(!landed[1].contains("alpha07.txt"));
    let rest = lp
        .session()
        .take_rest_chunk(usize::MAX, 2)
        .expect("the landing's own remainder is held");
    assert_eq!(
        rest.len(),
        37,
        "a landing that renumbered the index drops the old remainder"
    );
    for (name, _) in &rest {
        assert_ne!(name, "alpha07.txt");
        assert!(
            root.join("scope").join(name).is_file(),
            "{name} names a file still on disk"
        );
    }
    let answer = lp.session().re_answer_now().unwrap();
    assert_eq!(answer.total, 39);
    lp.close();
}

#[test]
fn search_landing_discarded_by_a_newer_query_never_restores_renumbered_rest() {
    let root = TestDir::new("search-landing-discard");
    root.dir("scope");
    for i in 0..40 {
        root.file(&format!("scope/alpha{i:02}.txt"), "a");
    }
    let mut lp = Loop::open(&root, "alpha", 2);
    assert_eq!(lp.session().pending_rest_len(), 38);
    let gone = root.join("scope/alpha07.txt");
    root.assert_contains(&gone);
    std::fs::remove_file(&gone).unwrap();
    lp.delta();
    lp.session().announced = false;
    lp.route(Event::SearchWalked);
    lp.route(Event::Request(
        r#"{"c":"searchquery","query":"alpha","gen":2,"count":2}"#.into(),
    ));
    lp.ready();
    assert!(
        lp.out.is_empty(),
        "the superseded landing answer is never published"
    );
    assert_eq!(
        lp.session().pending_rest_len(),
        0,
        "a landing that renumbered the index drops the old remainder"
    );
    lp.close();
}
