// Each re-sort names its anchor and each reply answers its new index; see docs/protocol.md "sort".
use super::*;
use crate::tui::model::Model;
use crate::tui::{actions, input::Key, keymap::Map};
use std::path::PathBuf;
use std::time::Duration;

// A wait for a line that never comes; only a broken echo child reaches it.
const FENCE_WAIT: Duration = Duration::from_secs(10);

// Everything the model has sent: the echo returns a fence line after it, so no quiet window can cut it short.
pub(crate) fn drain(wire: &mut Wire) -> Vec<Json> {
    wire.send(vec![("c", word("fence"))]).unwrap();
    let mut out = Vec::new();
    loop {
        let value = wire.events.recv_timeout(FENCE_WAIT).expect("the echo wire never returned its fence");
        let value = value.expect("backend line must parse");
        if text(&value, "c") == "fence" {
            return out;
        }
        out.push(value);
    }
}

pub(crate) fn requests<'a>(sent: &'a [Json], command: &str) -> Vec<&'a Json> {
    sent.iter().filter(|value| text(value, "c") == command).collect()
}

pub(super) fn press(model: &mut Model, wire: &mut Wire, key: &Key) {
    actions::key(model, key, &Map::load(), wire).unwrap();
}

fn sort_key() -> Key {
    let key = Key::character('s', "");
    assert_eq!(Map::load().action(&key, "default"), "sortNext", "the test presses the shipped sort key");
    key
}

fn reverse_key() -> Key {
    let key = Key { name: "S".into(), text: "S".into(), mods: "".into(), pointer: None };
    assert_eq!(Map::load().action(&key, "default"), "sortReverse", "the test presses the shipped reverse key");
    key
}

// A live listing through the real receive path: open, listed, rows. No model state by hand.
fn open_names(model: &mut Model, wire: &mut Wire, names: &[(&str, bool)]) {
    model.open(model.path.clone(), wire).unwrap();
    let sent = drain(wire);
    assert_eq!(requests(&sent, "list").len(), 1, "open sends one list");
    let n = names.len();
    model.receive(jsondoc::parse(&format!(r#"{{"t":"listed","n":{n},"read":0.0,"sort":0.0,"v":1,"path":"/listing"}}"#)).unwrap(), wire).unwrap();
    let sent = drain(wire);
    assert!(requests(&sent, "window").len() >= 1, "listed answers with a window fetch");
    let rows = names.iter().map(|(name, dir)| {
        if *dir { format!(r#"{{"n":"{name}","d":true}}"#) } else { format!(r#"{{"n":"{name}"}}"#) }
    }).collect::<Vec<_>>().join(",");
    model.receive(jsondoc::parse(&format!(r#"{{"t":"rows","start":0,"rows":[{rows}]}}"#)).unwrap(), wire).unwrap();
    assert!(drain(wire).is_empty(), "a plain rows reply sends nothing");
}

fn listed(n: usize, path: &str, anchor: &str, index: &str) -> Json {
    jsondoc::parse(&format!(r#"{{"t":"listed","n":{n},"read":0.0,"sort":0.0,"v":1,"path":"{path}","anchor":"{anchor}","anchorIndex":{index}}}"#)).unwrap()
}

fn plain_rows(names: &[&str]) -> Json {
    let rows = names.iter().map(|name| format!(r#"{{"n":"{name}"}}"#)).collect::<Vec<_>>().join(",");
    jsondoc::parse(&format!(r#"{{"t":"rows","start":0,"rows":[{rows}]}}"#)).unwrap()
}

#[test]
fn sort_burst_before_any_reply_ends_on_the_original_file() {
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    open_names(&mut model, &mut wire, &[("charlie.txt", false), ("amber", false), ("bronze", false)]);
    assert_eq!(model.current_path(), Some(PathBuf::from("/listing/charlie.txt")));
    // sss before any reply: size, then date, then kind, every one naming the same anchor.
    let mut by = Vec::new();
    for _ in 0..3 {
        press(&mut model, &mut wire, &sort_key());
        let sent = drain(&mut wire);
        let sorts = requests(&sent, "sort");
        assert_eq!(sorts.len(), 1, "each press sends exactly one re-sort");
        assert_eq!(text(sorts[0], "anchor"), "/listing/charlie.txt", "a press names the cursor's file");
        by.push(text(sorts[0], "by").to_string());
    }
    assert_eq!(by, vec!["size", "date", "kind"]);
    // The replies land in send order; each moves the cursor in its own order, the last wins.
    model.receive(listed(3, "/listing", "/listing/charlie.txt", "2"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, 2);
    model.receive(listed(3, "/listing", "/listing/charlie.txt", "0"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, 0);
    model.receive(listed(3, "/listing", "/listing/charlie.txt", "1"), &mut wire).unwrap();
    drain(&mut wire);
    model.receive(plain_rows(&["bronze", "charlie.txt", "amber"]), &mut wire).unwrap();
    assert!(drain(&mut wire).is_empty());
    assert_eq!(model.cursor, 1);
    assert_eq!(model.current_path(), Some(PathBuf::from("/listing/charlie.txt")), "the final order holds the original file");
    assert_eq!(model.sort_anchor, Some(PathBuf::from("/listing/charlie.txt")), "the wait survives its own replies, so a gap press still chains");
    finish(wire, reader);
}

#[test]
fn press_in_the_listed_rows_gap_chains_the_waiting_anchor() {
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    open_names(&mut model, &mut wire, &[("amber", false), ("bronze", false), ("charlie.txt", false)]);
    press(&mut model, &mut wire, &sort_key());
    let sent = drain(&mut wire);
    assert_eq!(text(requests(&sent, "sort")[0], "anchor"), "/listing/amber");
    // The re-sort's listed answered but its rows have not: the rows map is empty.
    model.receive(listed(3, "/listing", "/listing/amber", "2"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, 2);
    assert!(model.current_path().is_none(), "no row is loaded in the gap");
    // S in the gap cannot read a row, so it chains the anchor the first press named.
    press(&mut model, &mut wire, &reverse_key());
    let sent = drain(&mut wire);
    let sorts = requests(&sent, "sort");
    assert_eq!(sorts.len(), 1);
    assert_eq!(text(sorts[0], "anchor"), "/listing/amber", "a gap press keeps the logical row, never nothing");
    assert_eq!(text(sorts[0], "by"), "size");
    assert!(flag(sorts[0], "desc"), "the second press is the reverse");
    model.receive(plain_rows(&["bronze", "charlie.txt", "amber"]), &mut wire).unwrap();
    assert_eq!(model.cursor, 2, "rows for the first order move nothing by themselves");
    model.receive(listed(3, "/listing", "/listing/amber", "0"), &mut wire).unwrap();
    drain(&mut wire);
    model.receive(plain_rows(&["amber", "charlie.txt", "bronze"]), &mut wire).unwrap();
    assert_eq!(model.current_path(), Some(PathBuf::from("/listing/amber")), "the last reply holds the original file");
    finish(wire, reader);
}

#[test]
fn stale_listing_inputs_leave_a_late_sort_reply_alone() {
    // A re-sort empties the rows, so Return has none to open; Backspace needs none, and its parent listing must not move.
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    open_names(&mut model, &mut wire, &[("amber", false), ("sub", true)]);
    press(&mut model, &mut wire, &sort_key());
    let sent = drain(&mut wire);
    assert_eq!(text(requests(&sent, "sort")[0], "anchor"), "/listing/amber");
    press(&mut model, &mut wire, &Key::named("Backspace", ""));
    let sent = drain(&mut wire);
    assert_eq!(requests(&sent, "list").len(), 1, "Backspace navigates to the parent");
    assert!(model.sort_anchor.is_none(), "a navigation spends the outstanding anchor");
    model.receive(jsondoc::parse(r#"{"t":"listed","n":1,"read":0.0,"sort":0.0,"v":1,"path":"/"}"#).unwrap(), &mut wire).unwrap();
    drain(&mut wire);
    model.receive(plain_rows(&["only"]), &mut wire).unwrap();
    assert_eq!((model.path.clone(), model.cursor), (PathBuf::from("/"), 0));
    // The re-sort's reply arrives after the navigation completed: no wait, no move.
    model.receive(listed(2, "/listing", "/listing/amber", "1"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, 0, "the late re-sort reply moves nothing");
    assert!(model.sort_anchor.is_none());
    finish(wire, reader);

    // Directory change: open() itself spends the anchor.
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    open_names(&mut model, &mut wire, &[("amber", false), ("bronze", false)]);
    press(&mut model, &mut wire, &sort_key());
    drain(&mut wire);
    model.open(PathBuf::from("/other"), &mut wire).unwrap();
    let sent = drain(&mut wire);
    assert_eq!(text(requests(&sent, "list")[0], "path"), "/other");
    assert!(model.sort_anchor.is_none(), "a directory change spends the outstanding anchor");
    model.receive(jsondoc::parse(r#"{"t":"listed","n":1,"read":0.0,"sort":0.0,"v":1,"path":"/other"}"#).unwrap(), &mut wire).unwrap();
    drain(&mut wire);
    model.receive(plain_rows(&["zzz"]), &mut wire).unwrap();
    assert_eq!(model.current_path(), Some(PathBuf::from("/other/zzz")));
    model.receive(listed(2, "/listing", "/listing/amber", "1"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, 0, "the late re-sort reply moves nothing");
    finish(wire, reader);

    // Hidden toggle: the reload it sends spends the anchor too.
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    open_names(&mut model, &mut wire, &[("amber", false), ("bronze", false)]);
    press(&mut model, &mut wire, &sort_key());
    drain(&mut wire);
    assert_eq!(model.sort_anchor, Some(PathBuf::from("/listing/amber")), "no cursor key in between, so only the toggle can spend it");
    press(&mut model, &mut wire, &Key::character('.', ""));
    let sent = drain(&mut wire);
    assert!(model.hidden, "the toggle landed");
    assert_eq!(requests(&sent, "list").len(), 1, "the toggle reloads the listing");
    assert!(model.sort_anchor.is_none(), "a hidden toggle spends the outstanding anchor");
    model.receive(listed(2, "/listing", "/listing/amber", "1"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, 0, "the late re-sort reply moves nothing");
    finish(wire, reader);

    // Refresh: same stale rule through the path refresh() shares with open().
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    open_names(&mut model, &mut wire, &[("amber", false), ("bronze", false)]);
    press(&mut model, &mut wire, &sort_key());
    drain(&mut wire);
    model.refresh(&mut wire).unwrap();
    let sent = drain(&mut wire);
    assert_eq!(requests(&sent, "list").len(), 1, "refresh re-lists");
    assert!(model.sort_anchor.is_none(), "a refresh spends the outstanding anchor");
    model.receive(listed(2, "/listing", "/listing/amber", "1"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, 0, "the late re-sort reply moves nothing");
    finish(wire, reader);

    // Search: the walk replaces the listing, so the anchor goes with it.
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    open_names(&mut model, &mut wire, &[("amber", false), ("bronze", false)]);
    press(&mut model, &mut wire, &sort_key());
    drain(&mut wire);
    press(&mut model, &mut wire, &Key::character('f', ""));
    assert!(model.editor.is_some(), "f opens the search editor");
    press(&mut model, &mut wire, &Key::character('q', ""));
    press(&mut model, &mut wire, &Key::named("Return", ""));
    let sent = drain(&mut wire);
    assert_eq!(requests(&sent, "searchopen").len(), 1, "Return opens the ranked search session");
    assert_eq!(requests(&sent, "searchquery").len(), 1, "Return runs the search query");
    assert!(model.sort_anchor.is_none(), "a search spends the outstanding anchor");
    model.receive(listed(2, "/listing", "/listing/amber", "1"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, 0, "the late re-sort reply moves nothing");
    finish(wire, reader);
}

#[test]
fn gone_anchor_clears_and_never_sticks() {
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    open_names(&mut model, &mut wire, &[("a", false), ("b", false), ("c", false)]);
    press(&mut model, &mut wire, &Key::named("Down", ""));
    drain(&mut wire);
    press(&mut model, &mut wire, &Key::named("Down", ""));
    drain(&mut wire);
    assert_eq!(model.cursor, 2);
    press(&mut model, &mut wire, &sort_key());
    let sent = drain(&mut wire);
    assert_eq!(text(requests(&sent, "sort")[0], "anchor"), "/listing/c");
    // -1: the anchor is gone, so the anchor is spent and the clamped index stands.
    model.receive(listed(3, "/listing", "/listing/c", "-1"), &mut wire).unwrap();
    drain(&mut wire);
    assert!(model.sort_anchor.is_none(), "-1 spends the anchor");
    assert_eq!(model.cursor, 2);
    model.receive(plain_rows(&["a", "b", "c"]), &mut wire).unwrap();
    assert_eq!(model.current_path(), Some(PathBuf::from("/listing/c")));
    // A later reply naming the spent anchor must not yank the cursor back.
    model.receive(listed(3, "/listing", "/listing/c", "0"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, 2, "a spent anchor never moves the cursor again");
    // A shrunken listing clamps instead of following the ghost.
    model.receive(listed(1, "/listing", "/listing/c", "-1"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, 0);
    finish(wire, reader);
}

#[test]
fn reply_for_another_directory_is_ignored_and_the_wait_survives() {
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    open_names(&mut model, &mut wire, &[("amber", false), ("bronze", false)]);
    press(&mut model, &mut wire, &sort_key());
    drain(&mut wire);
    model.receive(listed(2, "/other", "/listing/amber", "1"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, 0, "a foreign reply moves nothing");
    assert_eq!(model.sort_anchor, Some(PathBuf::from("/listing/amber")), "the wait survives a foreign reply");
    model.receive(listed(2, "/listing", "/listing/amber", "1"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, 1, "the real reply still applies");
    finish(wire, reader);
}

#[test]
fn a_cursor_key_spends_the_anchor_so_a_late_reply_moves_nothing() {
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    open_names(&mut model, &mut wire, &[("amber", false), ("bronze", false), ("charlie.txt", false)]);
    press(&mut model, &mut wire, &sort_key());
    drain(&mut wire);
    assert_eq!(model.sort_anchor, Some(PathBuf::from("/listing/amber")));
    // The operator moves on before the reply: j is their choice of row, and the reply must not undo it.
    press(&mut model, &mut wire, &Key::character('j', ""));
    drain(&mut wire);
    assert_eq!(model.sort_anchor, None, "a cursor key spends the waiting anchor");
    let moved = model.cursor;
    model.receive(listed(3, "/listing", "/listing/amber", "2"), &mut wire).unwrap();
    drain(&mut wire);
    assert_eq!(model.cursor, moved, "the late reply leaves the cursor where the operator put it");
    finish(wire, reader);
}

#[test]
fn size_sort_requests_the_tui_folder_name_policy_and_other_sorts_do_not() {
    let (mut wire, reader) = super::echo_wire();
    let mut model = Model::new("/listing".into(), &Json::Null);
    open_names(&mut model, &mut wire, &[("folder", true), ("note.txt", false)]);
    press(&mut model, &mut wire, &sort_key());
    let sent = drain(&mut wire);
    assert!(flag(requests(&sent, "sort")[0], "foldersByName"));
    model.open("/other".into(), &mut wire).unwrap();
    let sent = drain(&mut wire);
    assert!(flag(requests(&sent, "list")[0], "foldersByName"));
    model.pending = None;
    press(&mut model, &mut wire, &sort_key());
    let sent = drain(&mut wire);
    assert!(!flag(requests(&sent, "sort")[0], "foldersByName"));
    super::finish(wire, reader);
}

#[test]
fn size_sort_keeps_folders_alphabetic_ahead_of_size_sorted_files_in_both_directions() {
    use crate::backend::{listing::Listing, mime::Db, ordering, testdir::TestDir};
    let root = TestDir::new("t4-size-policy");
    root.dir("A"); root.dir("Z"); root.file("A/large", &"x".repeat(4096));
    root.file("small.txt", "1"); root.file("large.txt", "123456789");
    for desc in [false, true] {
        let mut listing = Listing::new();
        for (name, folder) in [("Z", true), ("large.txt", false), ("A", true), ("small.txt", false)] { listing.push(name, folder); }
        let request = format!(r#"{{"c":"sort","by":"size","desc":{desc},"foldersFirst":false,"groupByKind":true,"foldersByName":true}}"#);
        let (_, _, seed) = ordering::request(&mut listing, root.path(), &Db::from_str(""), &request).unwrap();
        let names: Vec<_> = (0..listing.len()).map(|i| listing.name(i)).collect();
        assert_eq!(names, if desc { vec!["A", "Z", "large.txt", "small.txt"] } else { vec!["A", "Z", "small.txt", "large.txt"] });
        assert!(seed.is_empty(), "sorting does not recursively walk folders to compare their sizes");
    }
    let mut legacy = Listing::new(); legacy.push("A", true); legacy.push("Z", true);
    ordering::request(&mut legacy, root.path(), &Db::from_str(""), r#"{"c":"sort","by":"size"}"#).unwrap();
    assert_eq!(legacy.name(0), "Z", "a GUI request retains its existing walked-size order");
}
