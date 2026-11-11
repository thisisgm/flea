// tests/js/searchnoblank.js's exit and frame cases, search.js's paste and scope-join cases, and the new negatives.
use super::session_tests::{pane, sent, typing};
use super::*;

#[test]
fn a_frame_is_blank_only_when_nothing_answers_a_named_count() {
    use Frame::*;
    // searchnoblank.js:11, searchnoblank.js:13, searchnoblank.js:15
    assert_eq!(frame_kind(true, true, false, 0, 0, false), Blank);
    assert_eq!(frame_kind(true, true, true, 0, 0, false), Loading);
    assert_eq!((frame_kind(true, false, false, 5, 40, false), frame_kind(false, false, false, 5, 40, false)), (Drawn, Drawn));
    // searchnoblank.js:18, searchnoblank.js:20, searchnoblank.js:22
    assert_eq!(frame_kind(false, false, false, 3, 3, true), Drawn);
    assert_eq!(frame_kind(false, false, false, 0, 3, true), Blank);
    assert_eq!(frame_kind(false, false, false, 0, 0, true), Drawn);
    // searchnoblank.js:25, searchnoblank.js:29 (the held seven drawn), searchnoblank.js:32, searchnoblank.js:35
    assert_eq!(frame_kind(false, false, false, 40, 40, true), Drawn);
    assert_eq!(frame_kind(false, false, false, 7, 33, true), Drawn);
    assert_eq!(frame_kind(false, false, false, 0, 40, true), Blank);
    assert_eq!(frame_kind(false, false, false, 0, 40, false), Drawn);
    // searchnoblank.js:42, searchnoblank.js:50, searchnoblank.js:52 on the held seven
    assert_eq!(frame_kind(false, false, false, 7, 7, true), Drawn);
    assert_eq!(frame_kind(false, false, false, 0, 7, true), Blank);
    // searchnoblank.js:68, searchnoblank.js:70
    assert_eq!(frame_kind(true, true, false, 0, 12, false), Blank);
    assert_eq!(frame_kind(true, false, false, 3, 12, true), Drawn);
}

#[test]
fn the_abort_closes_once_and_relists_holding_the_walk() {
    let p = pane("/d", "");
    let mut exiting = Session { query: "nor".into(), gen: 9, from: "/d".into(), base: "/scope".into(), total: 12, ..typing("nor") };
    let out = exiting.abort(&p);
    // searchnoblank.js:80, searchnoblank.js:81 (Relist is the held exit), searchnoblank.js:82, searchnoblank.js:83
    assert!(out.contains(&Effect::Relist { path: "/d".into(), select: String::new() }));
    assert_eq!(sent(&out), ["close"]);
    assert_eq!((exiting.mode, exiting.leaving), (Mode::Typing, true));
    // A second Esc lands nowhere: the first already owes the landing.
    assert!(exiting.abort(&p).is_empty());
    assert!(exiting.type_key(&p, Key::Text("x")).is_empty(), "keys struck during the exit wait for the folder");
    exiting.close_leaving();
    assert_eq!((exiting.mode, exiting.query.as_str(), exiting.base.as_str()), (Mode::Off, "", ""));
}

#[test]
fn leaving_a_walk_defers_the_drawn_close_while_results_stand() {
    let mut living = Session { mode: Mode::Results, query: "n".into(), gen: 4, total: 3, ..Session::default() };
    // searchnoblank.js:103, searchnoblank.js:104
    assert_eq!(sent(&living.leave_walk()), ["close"]);
    assert_eq!((living.mode, living.leaving), (Mode::Results, true));
    // searchnoblank.js:110
    assert!(living.leave_walk().is_empty());
    let mut emptied = Session { gen: 2, total: 7, from: "/d".into(), ..typing("") };
    // searchnoblank.js:116
    assert_eq!(sent(&emptied.leave_walk()), ["close"]);
    assert_eq!(emptied.mode, Mode::Off);
    // A session that is not open closes nothing.
    assert!(Session::default().leave_walk().is_empty());
}

#[test]
fn a_running_walk_is_cancelled_before_esc_leaves() {
    let p = pane("/d", "");
    let mut walking = Session { running: true, gen: 3, from: "/d".into(), ..typing("abc") };
    assert_eq!(sent(&walking.type_key(&p, Key::Escape)), ["cancel"]);
    assert_eq!(walking.mode, Mode::Typing, "the first Esc keeps the results on screen");
    walking.searched(&Line::Searched { n: 5, scanned: 90, ms: 3., cancelled: true, gen: 3, walking: false, partial: None }, 7);
    assert_eq!((walking.cancelled, walking.running), (true, false));
    let out = walking.type_key(&p, Key::Escape);
    assert!(out.contains(&Effect::Relist { path: "/d".into(), select: String::new() }));
}

#[test]
fn a_query_from_outside_lands_as_results_over_home() {
    let p = pane("/home/u/Work", "/home/u");
    let mut session = Session { scope: Scope::Folder, ..Session::default() };
    let out = session.open_query(&p, "tag:Red");
    assert_eq!(sent(&out), ["open /home/u|/home/u/Work", "query 1:tag:Red/200"]);
    assert_eq!((session.mode, session.scope), (Mode::Results, Scope::Home));
    let mut cancelled = Session { cancelled: true, gen: 5, ..typing("x") };
    let out = cancelled.open_query(&p, "tag:Blue");
    assert_eq!(sent(&out), ["open /home/u|/home/u/Work", "query 6:tag:Blue/200"]);
    assert_eq!(cancelled.opened_gen, 5);
    let mut here = Session { scope: Scope::Folder, gen: 2, ..typing("old") };
    assert_eq!(sent(&here.open_query(&p, "tag:Red")), ["open /home/u|/home/u/Work", "query 3:tag:Red/200"]);
    let mut leaving = Session { leaving: true, ..typing("x") };
    assert!(leaving.open_query(&p, "tag:Red").is_empty(), "a leaving session takes no query");
}

#[test]
fn paste_types_the_first_clean_line_as_one_generation() {
    let p = pane("/d", "");
    let mut line = typing("");
    let out = line.type_key(&p, Key::Paste(Some("repo\nsecond line")));
    // search.js:319, search.js:320, search.js:324 (both chords arrive as Key::Paste)
    assert_eq!((line.query.as_str(), sent(&out)), ("repo", vec!["query 1:repo/200".to_owned()]));
    let mut dirty = typing("");
    dirty.type_key(&p, Key::Paste(Some("a\tb\u{1}c")));
    // search.js:327
    assert_eq!(dirty.query, "abc");
    let mut blank = typing("");
    // search.js:330, search.js:334
    assert!(blank.type_key(&p, Key::Paste(Some(""))).is_empty());
    assert!(blank.type_key(&p, Key::Paste(None)).is_empty());
    assert_eq!(clean_paste("x\r\ny"), "x");
}

#[test]
fn rows_join_against_the_scope_and_reveal_opens_their_folder() {
    let scoped = Session { base: "/home/u".into(), ..typing("notes") };
    // search.js:276, search.js:288, search.js:296: menu targets and the drag source join against the scope.
    assert_eq!(super::session::join(scoped.base_of("/home/u/Downloads/deep"), "docs/notes-old.md"), "/home/u/docs/notes-old.md");
    assert_eq!(scoped.base_of("/home/u/Downloads/deep"), "/home/u");
    let mut session = scoped.clone();
    let out = session.reveal(&pane("/home/u/Downloads/deep", "/home/u"), Some("docs/notes-old.md"));
    assert_eq!(out, [Effect::Reveal { folder: "/home/u/docs".into(), select: "/home/u/docs/notes-old.md".into() }]);
    assert_eq!(session.pending_select, "/home/u/docs/notes-old.md");
    assert!(session.reveal(&pane("/d", ""), None).is_empty(), "no row reveals nothing");
    let mut root = Session::default();
    assert!(root.reveal(&pane("/", ""), Some("a.txt")).is_empty(), "a file in the root has no folder to open");
}

#[test]
fn a_pushed_answer_for_the_same_query_still_needs_the_newest_gen() {
    let p = pane("/d", "");
    let mut session = Session::default();
    session.start(&p);
    session.type_key(&p, Key::Text("a"));
    session.searched(&Line::Searched { n: 2, scanned: 10, ms: 1., cancelled: false, gen: 1, walking: true, partial: None }, 5);
    // The walk's landing re-answers the standing query under its own gen, so the counts move with no keystroke.
    session.searched(&Line::Searched { n: 3, scanned: 50, ms: 9., cancelled: false, gen: 1, walking: false, partial: None }, 6);
    assert_eq!((session.total, session.scanned, session.running), (3, 50, false));
    // An edited query that adds a match lands only once its own answer names its gen.
    session.type_key(&p, Key::Backspace);
    session.searched(&Line::Searched { n: 7, scanned: 50, ms: 1., cancelled: false, gen: 1, walking: false, partial: None }, 7);
    assert_eq!(session.total, 3, "the old gen's answer is stale once a newer query is out");
    session.searched(&Line::Searched { n: 0, scanned: 50, ms: 1., cancelled: false, gen: 2, walking: false, partial: None }, 8);
    assert_eq!((session.total, session.listing_state(0)), (0, ListingState::Empty), "a zero-count answer is an honest empty");
}

#[test]
fn cancel_then_reopen_starts_a_fresh_numbering_floor() {
    let p = pane("/d", "");
    let mut session = Session::default();
    session.start(&p);
    session.type_key(&p, Key::Text("q"));
    session.type_key(&p, Key::Escape);
    session.searched(&Line::Searched { n: 1, scanned: 4, ms: 1., cancelled: true, gen: 1, walking: false, partial: None }, 3);
    session.type_key(&p, Key::Escape);
    session.close_leaving();
    let out = session.start(&p);
    assert_eq!((session.opened_gen, sent(&out)), (1, vec!["open /d|/d".to_owned()]));
    assert!(!session.accepts(1), "the cancelled session's gen never lands on the reopened one");
}

#[test]
fn contents_toggle_waits_for_its_producer() {
    let p = pane("/d", "");
    let mut plain = typing("word");
    assert!(plain.type_key(&p, Key::Contents).is_empty());
    assert_eq!(plain.kind, Kind::Names);
    let mut wired = Session { wire: Wire { everywhere: false, contents: true }, ..typing("word") };
    assert_eq!(sent(&wired.type_key(&p, Key::Contents)), ["contents 1:word/200 in /d"]);
    assert_eq!(wired.kind, Kind::Contents);
}

#[test]
fn a_contents_query_asks_the_producer_and_never_the_names_walk() {
    let p = pane("/d", "");
    let mut wired = Session { wire: Wire { everywhere: true, contents: true }, ..typing("") };
    assert!(wired.type_key(&p, Key::Contents).is_empty(), "no query yet, so nothing is asked");
    assert_eq!(sent(&wired.type_key(&p, Key::Text("F"))), ["contents 1:F/200 in /d"]);
    assert_eq!(sent(&wired.type_key(&p, Key::Text("l"))), ["contents 2:Fl/200 in /d"]);
    assert_eq!(sent(&wired.type_key(&p, Key::Contents)), ["query 3:Fl/200"], "Ctrl+G back to names asks the names walk");
    assert_eq!(sent(&wired.type_key(&p, Key::Contents)), ["contents 4:Fl/200 in /d"], "Ctrl+G again asks the producer");
    wired.scope = Scope::Everywhere;
    assert!(wired.type_key(&p, Key::Text("e")).is_empty(), "contents has no root under Everywhere, so nothing is asked");
    assert_eq!((wired.query.as_str(), wired.gen), ("Fle", 4));
}

#[test]
fn the_answer_keeps_its_partial_reason_until_the_next_query() {
    let p = pane("/d", "");
    let mut session = Session::default();
    session.start(&p);
    session.type_key(&p, Key::Text("a"));
    let partial = Some("unreadable".to_owned());
    session.searched(&Line::Searched { n: 2, scanned: 10, ms: 1., cancelled: false, gen: 1, walking: false, partial }, 5);
    assert_eq!(session.partial.as_deref(), Some("unreadable"));
    session.type_key(&p, Key::Text("b"));
    assert_eq!(session.partial, None, "a newer query has no answer yet, so no partial reason");
    session.searched(&Line::Searched { n: 1, scanned: 10, ms: 1., cancelled: false, gen: 2, walking: false, partial: None }, 6);
    assert_eq!((session.partial, session.scanned, session.running), (None, 10, false));
}

#[test]
fn leaving_the_search_forgets_which_gen_answered() {
    let p = pane("/d", "");
    let mut session = Session::default();
    session.start(&p);
    session.type_key(&p, Key::Text("a"));
    session.searched(&Line::Searched { n: 2, scanned: 11, ms: 1., cancelled: false, gen: 1, walking: false, partial: None }, 5);
    assert_eq!(session.answered, 1);
    session.type_key(&p, Key::Escape);
    session.type_key(&p, Key::Escape);
    session.close_leaving();
    assert_eq!((session.answered, session.scanned), (0, 0), "cleared counts are no gen's answer");
    session.start(&p);
    assert_eq!(session.answered, 0, "a reopened walk has not answered yet");
}


// v0.3.12 drew an incomplete search as it drew a complete one, so only the kept reason differs once it lands.
#[test]
fn a_partial_landing_lands_as_the_complete_one_does() {
    let p = pane("/d", "");
    let land = |partial: Option<String>| {
        let mut session = Session::default();
        session.start(&p);
        session.type_key(&p, Key::Text("a"));
        let line = Line::Searched { n: 3, scanned: 90, ms: 4., cancelled: false, gen: 1, walking: false, partial };
        let effects = session.searched(&line, 5);
        assert_eq!(session.answered, 1, "the line lands on the live gen");
        let reason = session.partial.take();
        (format!("{effects:?} {session:?}"), reason)
    };
    let (complete, reason) = land(None);
    assert_eq!(reason, None, "a complete answer keeps no reason");
    assert_eq!(land(Some("unreadable".into())), (complete, Some("unreadable".into())));
}
