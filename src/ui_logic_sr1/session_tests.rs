// tests/js/search.js's live-session cases, each citing the oracle line it ports.
use super::*;

pub(super) fn pane<'a>(path: &'a str, home: &'a str) -> Pane<'a> {
    Pane { path, home, hidden: false, count: 200, sizes: 0, cursor_name: Some("todo.txt"), recent: None, now: 1000 }
}

// The wire as search.js's stub records it: "open scope|folder", "query gen:text/count", "close", "cancel".
pub(super) fn sent(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Send(Request::Open { scope, folder, .. }) => Some(format!("open {scope}|{folder}")),
            Effect::Send(Request::Query { query, gen, count, .. }) => Some(format!("query {gen}:{query}/{count}")),
            Effect::Send(Request::Contents { path, query, gen, count, .. }) => Some(format!("contents {gen}:{query}/{count} in {path}")),
            Effect::Send(Request::Close) => Some("close".into()),
            Effect::Send(Request::Cancel) => Some("cancel".into()),
            _ => None,
        })
        .collect()
}

pub(super) fn typing(query: &str) -> Session {
    Session { mode: Mode::Typing, query: query.into(), ..Session::default() }
}

fn searched(n: usize, gen: u64, walking: bool) -> Line {
    Line::Searched { n, scanned: 400, ms: 33.1, cancelled: false, gen, walking, partial: None }
}

#[test]
fn opening_starts_the_scope_walk_once_and_arms_the_way_back() {
    let p = pane("/d", "");
    let mut opener = Session::default();
    let out = opener.start(&p);
    // search.js:97
    assert_eq!((opener.mode, sent(&out)), (Mode::Typing, vec!["open /d|/d".to_owned()]));
    // search.js:99, search.js:252
    assert_eq!((opener.from.as_str(), opener.pending_select.as_str()), ("/d", "/d/todo.txt"));
    // search.js:104
    let mut again = typing("");
    assert!(again.start(&p).is_empty(), "opening twice starts one session");
}

#[test]
fn opening_from_recent_leaves_it_first_and_asks_the_folder_rows() {
    let p = Pane { recent: Some("/d/sub"), ..pane("/", "") };
    let mut session = Session::default();
    let out = session.start(&p);
    // search.js:111
    assert_eq!(out[0], Effect::LeaveRecent { path: "/d/sub".into() });
    assert_eq!(sent(&out), ["open /d/sub|/d/sub", "query 1:/200"]);
    // search.js:114
    assert_eq!(session.pending_select, "/todo.txt");
    // search.js:115
    assert_eq!(session.base_of("/d/sub"), "/");
    // search.js:118: Esc with nothing typed re-lists the folder over Recent's rows.
    let back = Pane { recent: None, ..pane("/d/sub", "") };
    let out = session.type_key(&back, Key::Escape);
    assert_eq!(session.mode, Mode::Off);
    assert!(out.contains(&Effect::Relist { path: "/d/sub".into(), select: "/todo.txt".into() }));
}

#[test]
fn keys_extend_shrink_and_rise_one_generation_each() {
    let p = pane("/d", "");
    let mut del = typing("");
    // search.js:122
    assert!(del.type_key(&p, Key::Text("\u{7f}")).is_empty());
    assert_eq!(del.query, "");
    // search.js:124
    assert_eq!(sent(&del.type_key(&p, Key::Text("e"))), ["query 1:e/200"]);
    let mut line = typing("scr");
    // search.js:128
    assert_eq!(sent(&line.type_key(&p, Key::Text("e"))), ["query 1:scre/200"]);
    // search.js:131
    assert_eq!(sent(&line.type_key(&p, Key::Text("e"))), ["query 2:scree/200"]);
    // search.js:132: a send moves no cursor; the effects name none.
    assert!(!line.type_key(&p, Key::Other).iter().any(|e| matches!(e, Effect::Reveal { .. } | Effect::Relist { .. })));
    // search.js:134
    assert_eq!((sent(&line.type_key(&p, Key::Backspace)), line.query.as_str()), (vec!["query 3:scre/200".to_owned()], "scre"));
    let mut last = Vec::new();
    for _ in 0..4 {
        last = line.type_key(&p, Key::Backspace);
    }
    // search.js:140
    assert_eq!((sent(&last), line.query.as_str(), line.mode), (vec!["query 7:/200".to_owned()], "", Mode::Typing));
    // search.js:143
    assert!(line.type_key(&p, Key::Backspace).is_empty());
}

#[test]
fn answers_land_only_for_the_newest_generation() {
    let mut keyed = typing("scr");
    keyed.gen = 2;
    keyed.total = 11;
    // search.js:148
    assert!(keyed.searched(&searched(7, 1, true), 5).is_empty());
    assert_eq!((keyed.total, keyed.running), (11, false));
    // search.js:150
    assert_eq!(keyed.searched(&searched(4, 2, true), 5), [Effect::RestartSettle]);
    assert_eq!((keyed.total, keyed.running), (4, true));
    // search.js:153
    keyed.searched(&searched(4, 2, false), 5);
    assert!(!keyed.running);
}

#[test]
fn a_closed_sessions_late_answer_never_lands_on_the_next() {
    let p = pane("/d", "");
    let mut reopened = typing("scr");
    reopened.gen = 1;
    reopened.total = 11;
    reopened.close_session();
    reopened.start(&p);
    reopened.searched(&searched(7, 1, true), 5);
    // search.js:160
    assert_eq!((reopened.total, reopened.running), (11, true));
    let out = reopened.type_key(&p, Key::Text("s"));
    reopened.searched(&searched(7, 1, true), 5);
    // search.js:164
    assert_eq!((reopened.total, sent(&out)), (11, vec!["query 2:s/200".to_owned()]));
    let mut idle = typing("scr");
    idle.gen = 3;
    idle.close_session();
    idle.start(&p);
    let out = idle.type_key(&p, Key::Escape);
    // search.js:171
    assert_eq!(idle.mode, Mode::Off);
    assert!(!out.iter().any(|e| matches!(e, Effect::Relist { .. })));
}

#[test]
fn enter_keeps_and_escape_returns() {
    let p = pane("/d", "");
    let mut kept = typing("scr");
    kept.gen = 1;
    // search.js:176
    assert!(kept.type_key(&p, Key::Enter).is_empty());
    assert_eq!(kept.mode, Mode::Results);
    let mut abandoned = typing("scr");
    abandoned.gen = 3;
    abandoned.from = "/d".into();
    let out = abandoned.type_key(&p, Key::Escape);
    // search.js:181: the strip stands until the folder lands.
    assert_eq!((abandoned.mode, sent(&out)), (Mode::Typing, vec!["close".to_owned()]));
    assert!(out.contains(&Effect::Relist { path: "/d".into(), select: String::new() }));
    let mut untouched = Session::default();
    let opened = untouched.start(&p);
    let out = untouched.type_key(&p, Key::Escape);
    // search.js:188
    assert_eq!(untouched.listing_state(0), ListingState::Empty);
    // search.js:190
    assert_eq!([sent(&opened), sent(&out)].concat(), ["open /d|/d", "close"]);
    assert!(!out.iter().any(|e| matches!(e, Effect::Relist { .. })));
    let mut blank = typing("");
    // search.js:233
    assert_eq!((sent(&blank.type_key(&p, Key::Enter)), blank.mode), (vec!["close".to_owned()], Mode::Off));
}

#[test]
fn the_scope_is_home_from_inside_it_and_tab_cycles_it() {
    let p = pane("/home/u/Downloads", "/home/u");
    let mut under = Session::default();
    let out = under.start(&p);
    // search.js:197
    assert_eq!((sent(&out), under.from.as_str()), (vec!["open /home/u|/home/u/Downloads".to_owned()], "/home/u/Downloads"));
    // search.js:33, search.js:35, search.js:36, search.js:38, search.js:40, search.js:204, search.js:206
    let root = |path, home, scope| Session::scope_root(path, home, scope).unwrap();
    assert_eq!(root("/home/u/Downloads/deep", "/home/u", Scope::Home), "/home/u");
    assert_eq!(root("/home/u", "/home/u", Scope::Home), "/home/u");
    assert_eq!(root("/run/media/u/NAS/photos", "/home/u", Scope::Home), "/run/media/u/NAS/photos");
    assert_eq!(root("/home/under", "/home/u", Scope::Home), "/home/under");
    assert_eq!(root("/d", "", Scope::Home), "/d");
    assert_eq!(root("/home/u/Downloads/deep", "/home/u", Scope::Folder), "/home/u/Downloads/deep");
    let mut here = typing("scr");
    here.gen = 1;
    let out = here.type_key(&p, Key::Tab);
    // search.js:213, search.js:214
    assert_eq!((here.scope, sent(&out)), (Scope::Folder, vec!["open /home/u/Downloads|/home/u/Downloads".to_owned(), "query 2:scr/200".into()]));
    let out = here.type_key(&p, Key::Tab);
    // search.js:217, search.js:229: with no everywhere producer Tab flips back to Home.
    assert_eq!((here.scope, sent(&out)[1].as_str()), (Scope::Home, "query 3:scr/200"));
    let mut empty = typing("");
    // search.js:221
    assert_eq!(sent(&empty.type_key(&pane("/d", ""), Key::Tab)), ["open /d|/d"]);
    let mut done = typing("kept");
    done.mode = Mode::Results;
    done.gen = 4;
    // search.js:227
    assert!(done.start(&p).is_empty());
    assert_eq!((done.mode, done.query.as_str()), (Mode::Typing, "kept"));
}

#[test]
fn tab_reaches_everywhere_only_with_its_producer() {
    let p = pane("/home/u/Downloads", "/home/u");
    let mut wired = Session::new(Wire { everywhere: true, contents: false });
    wired.start(&p);
    assert_eq!(wired.scope, Scope::Home);
    wired.type_key(&p, Key::Tab);
    assert_eq!(wired.scope, Scope::Everywhere);
    wired.type_key(&p, Key::Tab);
    assert_eq!(wired.scope, Scope::Folder);
    let mut plain = Session::default();
    plain.start(&p);
    for _ in 0..4 {
        plain.type_key(&p, Key::Tab);
        assert_ne!(plain.scope, Scope::Everywhere, "no everywhere shape exists at this base");
    }
}

#[test]
fn the_stopwatch_and_kept_index() {
    // search.js:236, search.js:238, search.js:239
    assert_eq!(Session::kept_index(Some("todo.txt"), ["b.txt", "todo.txt"], 0), 1);
    assert_eq!(Session::kept_index(Some("gone.txt"), ["b.txt"], 5), 5);
    assert_eq!(Session::kept_index(None, ["b.txt"], 5), 5);
    let p = pane("/d", "");
    let mut stamped = typing("");
    stamped.type_key(&p, Key::Text("e"));
    // search.js:242
    assert_eq!(stamped.stamps, Stamps { gen: 1, key_at: 1000, first_rows_at: 0, settled_at: 0 });
    stamped.rows_landed(1, 1002);
    stamped.searched(&Line::Searched { n: 1, scanned: 10, ms: 5.5, cancelled: false, gen: 1, walking: false, partial: None }, 1004);
    // search.js:245
    assert_eq!((stamped.stamps.first_rows_at, stamped.stamps.settled_at), (1002, 1004));
    stamped.searched(&Line::Searched { n: 9, scanned: 10, ms: 5.5, cancelled: false, gen: 0, walking: false, partial: None }, 2000);
    // search.js:248
    assert_eq!((stamped.total, stamped.stamps.settled_at), (1, 1004));
}

#[test]
fn a_new_session_restarts_the_landed_generation() {
    let p = pane("/d", "");
    let mut fresh = Session::default();
    // search.js:266
    assert!(fresh.start(&p).contains(&Effect::ResetLanded));
    let mut kept = typing("");
    kept.mode = Mode::Results;
    // search.js:261
    assert!(!kept.start(&p).contains(&Effect::ResetLanded));
}

#[test]
fn pointer_activation_and_listing_state() {
    let results = Session { mode: Mode::Results, query: "no".into(), ..Session::default() };
    // search.js:50, search.js:52, search.js:54, search.js:56
    assert!(results.reveals_on_activate());
    assert!(!Session::default().reveals_on_activate());
    assert!(!typing("").reveals_on_activate());
    assert!(typing("no").reveals_on_activate());
    let running = Session { running: true, ..Session::default() };
    // search.js:60, search.js:61, search.js:62
    assert_eq!(running.listing_state(3), ListingState::Ready);
    assert_eq!(running.listing_state(0), ListingState::Loading);
    assert_eq!(Session::default().listing_state(0), ListingState::Empty);
}
