//! Port of PickerPathBar.qml: peek, open, locate, say.
use super::chrome::PathBar;
use super::listing::{Listing, NameMatcher};
use super::route::*;
use crate::path::entry::Entry;
use crate::path::suggestions::{Ranker, Suggestion, Unranked};
use crate::path::view::{Inputs, Out};
use crate::path::{complete, completion_message, resolve, should_navigate, Said};
use flea::i18n::{I18n, Paths};
use flea::keys::{lookup, Context, Frontend, Input, Match};
use flea::protocol::RowFacts;
use gpui::{SharedString, TestAppContext};
use std::{cell::RefCell, rc::Rc};

fn english() -> I18n {
    let none = std::path::PathBuf::from("/nonexistent/flea-pk2-route");
    I18n::load(Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}

// pickerpath.js:14; the chrome builds its shared entry synchronously.
#[gpui::test]
fn the_main_chrome_holds_the_shared_entry(cx: &mut TestAppContext) {
    let (bar, cx) = cx.add_window_view(|_, cx| PathBar::new(&english(), cx));
    let shared = bar.read_with(cx, |bar, _| bar.path());
    bar.update(cx, |bar, cx| {
        bar.path()
            .update(cx, |view, _| assert!(view.entry.start("/d")));
    });
    assert_eq!(
        shared.read_with(cx, |view, _| view.entry.text.clone()),
        "/d/"
    );
    shared.update(cx, |view, _| view.entry.text = "/d/typed".into());
    let reread = bar.read_with(cx, |bar, _| bar.path());
    assert_eq!(
        reread, shared,
        "the bar returns the entity its renderer holds"
    );
    assert_eq!(
        reread.read_with(cx, |view, _| view.entry.text.clone()),
        "/d/typed"
    );
}

// pickerpath.js:16; the held view keeps its entry across syncs.
#[gpui::test]
fn the_picker_holds_the_shared_entry(cx: &mut TestAppContext) {
    let (bar, cx) = cx.add_window_view(|_, cx| PathBar::new(&english(), cx));
    bar.update(cx, |bar, cx| {
        bar.path().update(cx, |view, _| {
            assert!(view.entry.start("/d"), "the picker holds an entry");
        });
    });
    bar.update(cx, |bar, cx| {
        bar.sync(
            Inputs {
                paths: [SharedString::from("/e"), SharedString::default()],
                home: "/home/gm".into(),
                ..Inputs::default()
            },
            cx,
        );
        bar.path().update(cx, |view, _| {
            assert_eq!(view.entry.text, "/d/", "sync keeps the shared entry");
        });
    });
}

// pickerpath.js:18; the entry carries its text field.
#[test]
fn the_entry_carries_the_field() {
    let mut entry = Entry::default();
    assert!(entry.start("/d"), "starting opens the field");
    assert_eq!(entry.text, "/d/");
    assert!(entry.editing);
    assert!(!entry.start("/e"), "an open field is never rebuilt");
    assert!(entry.close());
}

// pickerpath.js:22; the chrome keeps resolving through base and home.
#[test]
fn the_chrome_keeps_its_resolve() {
    assert_eq!(resolve("docs", "/b", "/home/gm"), "/b/docs");
    assert_eq!(resolve("~/x", "/b", "/home/gm"), "/home/gm/x");
    assert_eq!(resolve("/a", "/b", "/home/gm"), "/a");
}

// pickerpath.js:24; a settled same path stays shut, a failure retries.
#[test]
fn the_chrome_keeps_its_retry_gate() {
    assert!(!should_navigate("/d", "/d", false), "settled: no retry");
    assert!(should_navigate("/e", "/d", false), "moved: navigate");
    assert!(should_navigate("/d", "/d", true), "failed: retry");
}

// pickerpath.js:26; the shared entry emits the refusal through its actual commit.
#[gpui::test]
fn the_chrome_says_the_shared_refusal_sentence(cx: &mut TestAppContext) {
    assert_eq!(
        commit_line("file://otherhost/x", "/home/gm", "/home/gm"),
        CommitAsk::Say(Said::RefusedUri, false)
    );
    commit_entry(
        cx,
        "file://otherhost/x",
        vec![Out::Closed, Out::Said(Said::RefusedUri.text(&english()))],
    );
}

// pickerpath.js:31; opening builds and focuses the shared entry before committing.
#[gpui::test]
fn the_chrome_delegates_its_open(cx: &mut TestAppContext) {
    assert_eq!(
        commit_line("/d", "/b", "/home/gm"),
        CommitAsk::Peek("/d".into())
    );
    assert_eq!(
        peeked("/d", "/d", false, 0o040755, false, false),
        Peeked::OpenFolder("/d".into())
    );
    commit_entry(
        cx,
        "/d",
        vec![
            Out::Closed,
            Out::Open {
                side: 0,
                path: "/d".into(),
            },
        ],
    );
}

fn commit_entry(cx: &mut TestAppContext, text: &str, expected: Vec<Out>) {
    let (bar, cx) = cx.add_window_view(|_, cx| PathBar::new(&english(), cx));
    let path = bar.read_with(cx, |bar, _| bar.path());
    let events = Rc::new(RefCell::new(Vec::new()));
    let recorded = events.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&path, move |_, out: &Out, _| {
            recorded.borrow_mut().push(out.clone())
        })
    });
    assert!(!path.read_with(cx, |view, _| view.built));
    bar.update(cx, |bar, cx| {
        bar.sync(
            Inputs {
                paths: ["/b".into(), SharedString::default()],
                home: "/home/gm".into(),
                ..Inputs::default()
            },
            cx,
        );
    });
    cx.update(|window, cx| path.update(cx, |view, cx| view.open(window, cx)));
    cx.run_until_parked();
    assert!(path.read_with(cx, |view, _| view.built && view.editing()));
    assert!(cx.update(|window, cx| path.read(cx).holds_keyboard(window)));
    let opened = std::mem::take(&mut *events.borrow_mut());
    assert!(
        opened
            .iter()
            .all(|out| matches!(out, Out::Send { .. } | Out::ReadHistory)),
        "opening asks the jump for its history and nothing else"
    );
    cx.simulate_input(text);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        *events.borrow(),
        expected,
        "the real entry must emit its host events"
    );
    assert!(!path.read_with(cx, |view, _| view.editing()));
}

// pickerpath.js:33; the entry stays shut until starting builds it.
#[test]
fn the_chrome_stays_shut_until_built() {
    let mut entry = Entry::default();
    assert!(!entry.editing, "shut before built");
    assert!(!entry.close(), "shut stays shut");
    assert!(entry.start("/d"), "starting builds it");
    assert!(entry.editing);
}

// pickerpath.js:36; Unranked never ranks, so the jump never opens.
#[test]
fn the_chrome_loads_no_jump_of_its_own() {
    let ranker = Unranked;
    assert!(!ranker.ranks(), "no ranker, no jump");
}

// pickerpath.js:39; Unranked leaves rows in listing order.
#[test]
fn the_picker_holds_no_ranking_of_its_own() {
    let ranker = Unranked;
    let mut rows = vec![
        Suggestion {
            path: "/d/b".into(),
            text: "b".into(),
            leaf_start: 3,
            wash: 0..1,
        },
        Suggestion {
            path: "/d/a".into(),
            text: "a".into(),
            leaf_start: 3,
            wash: 0..1,
        },
    ];
    ranker.rows("x", &mut rows);
    assert_eq!(
        rows.iter().map(|row| row.text.clone()).collect::<Vec<_>>(),
        ["b", "a"]
    );
}

// pickerpath.js:42; the bar resolves text against base and home.
#[test]
fn the_picker_resolves_through_pathbar() {
    assert_eq!(
        commit_line("docs", "/b", "/home/gm"),
        CommitAsk::Peek("/b/docs".into())
    );
    assert_eq!(
        commit_line("~/x", "/b", "/home/gm"),
        CommitAsk::Peek("/home/gm/x".into())
    );
}

// pickerpath.js:43; commit wraps resolve with nothing between.
#[test]
fn no_resolver_wrapper_remains() {
    for text in ["docs", "~/x", "d/f", "/a"] {
        assert_eq!(
            commit_line(text, "/b", "/h"),
            CommitAsk::Peek(resolve(text, "/b", "/h"))
        );
    }
}

// Helper coverage only; pickerpath.js:47 needs pk1's first == 0 reply dispatcher.
#[test]
fn tab_no_match_and_committed_line_use_different_helpers() {
    let tab = complete("zzz", &["a.txt"]);
    assert_eq!(tab.matches, 0, "Tab found nothing");
    assert_eq!(
        completion_message("zzz", &tab, "/d"),
        Some(Said::NothingStartsWith("/d".into())),
        "Tab says its own sentence"
    );
    assert_eq!(
        commit_line("zzz", "/d", "/h"),
        CommitAsk::Peek("/d/zzz".into()),
        "a line peeks"
    );
}

// pickerpath.js:48; opening a folder lists through the open.
#[test]
fn a_folder_lists_through_the_picker_s_own_open() {
    let mut list = Listing::default();
    list.open("/d");
    assert!(list.is_pending());
    assert!(
        list.request_line(0).contains("\"path\":\"/d\""),
        "the open asks for its path"
    );
}

// pickerpath.js:49; a file's parent rides the locate answer.
#[test]
fn a_file_s_parent_lists_through_the_picker_s_own_open() {
    assert_eq!(
        peeked("/d/a.txt", "/d/a.txt", true, 0o100644, false, false),
        Peeked::LocateFile {
            parent: "/d".into(),
            leaf: "/d/a.txt".into()
        }
    );
}

#[path = "route_ports_fixture.rs"]
mod fixture;
use fixture::landed_two;

struct Keep<'a>(&'a str);

impl NameMatcher for Keep<'_> {
    fn keeps(&mut self, name: &str) -> bool {
        name == self.0
    }
}

// Held-row hunt only; pickerpath.js:53 needs pk1's backend locate request.
#[test]
fn held_row_hunt_finds_a_name_in_its_window() {
    let mut list = landed_two();
    assert_eq!(list.hunt(&mut Keep("a.txt")), Some(1));
    assert_eq!(list.cursor(), 1);
}

// pickerpath.js:54; located answers from indices, never a window.
#[test]
fn no_window_is_walked_to_find_a_typed_file() {
    let mut list = Listing::default();
    list.open("/d");
    assert_eq!(list.window_request(0, 50), None, "no window walk yet");
    assert_eq!(
        located("/d/a.txt", "/d", "/d", "/d/a.txt", 7),
        Located::Row(7)
    );
}

// Direct cursor coverage only; pickerpath.js:55 needs pk1's locate reply handler.
#[test]
fn the_listing_accepts_a_direct_cursor_move() {
    let mut list = landed_two();
    list.move_cursor(1);
    assert_eq!(list.cursor(), 1);
}

// pickerpath.js:56; only the held window takes the row.
#[test]
fn the_located_row_s_window_is_the_view_s_own() {
    assert_eq!(take_row("/d/a.txt", 3, true), Some(3));
    assert_eq!(take_row("/d/a.txt", 3, false), None, "unheld takes nothing");
    let list = landed_two();
    assert_eq!(
        list.held().listing,
        list.generation() as usize,
        "the window is the listing's own"
    );
}

// pickerpath.js:69; both sentences render through the one helper.
#[test]
fn the_picker_says_both_through_the_helper() {
    assert_eq!(
        Said::Missing("/x/y z".into()).text(&english()),
        "/x/y z does not exist or cannot be reached."
    );
    assert_eq!(
        Said::Filtered("a.png".into()).text(&english()),
        "a.png is hidden by the file filter or the hidden files setting."
    );
}

// pickerpath.js:84; the listing context looks up the pathBar action.
#[test]
fn the_list_routes_the_pathbar_action() {
    let input = Input {
        key: "",
        text: ":",
        modifiers: 0,
    };
    assert_eq!(
        lookup("default", Context::Listing, Frontend::Gui, input),
        Match::Action("pathBar")
    );
}
