// Tabs040 callout 1 through the real key path: { and } move the current tab
// one place, clamped at either end, and a move sends nothing because the pane
// stays on its path.
use super::sort_tests::{drain, press};
use super::*;
use crate::tui::input::Key;
use crate::tui::keymap::Map;
use crate::tui::model::Model;
use std::path::PathBuf;

fn move_key(text: &str) -> Key {
    let key = Key::character(text.chars().next().unwrap(), "");
    let action = Map::load().action(&key, "default");
    assert_eq!(action, if text == "}" { "tabMoveRight" } else { "tabMoveLeft" },
        "the test presses the shipped reorder key");
    key
}

fn shift_page(up: bool) -> Key {
    let key = Key::named(if up { "PageUp" } else { "PageDown" }, "ctrlshift");
    let action = Map::load().action(&key, "default");
    assert_eq!(action, if up { "tabMoveLeft" } else { "tabMoveRight" },
        "the test presses the shipped reorder chord");
    key
}

fn cursors(model: &Model) -> Vec<usize> {
    model.tabs.iter().map(|tab| tab.cursor).collect()
}

// A tab switch opens the tab's path, so each new tab settles its listing
// before the next opens, the way sort_tests' open_names settles its one.
fn settle_open(model: &mut Model, wire: &mut Wire) {
    drain(wire);
    model.receive(crate::jsondoc::parse(r#"{"t":"listed","n":1,"read":0.0,"sort":0.0,"v":1,"path":"/listing"}"#).unwrap(), wire).unwrap();
    drain(wire);
    model.receive(crate::jsondoc::parse(r#"{"t":"rows","start":0,"rows":[{"n":"photo.jpg"}]}"#).unwrap(), wire).unwrap();
    drain(wire);
}

fn open_key() -> Key {
    let key = Key::named("Return", "ctrl");
    assert_eq!(Map::load().action(&key, "default"), "openTab", "the test presses the shipped open-tab chord");
    key
}

// A single listing row for the open-tab branches below.
fn folder_row() -> crate::tui::model::Row {
    crate::tui::model::Row { name: "sub".into(), directory: true, size: 0, mode: 0o40755, link: String::new(), kind: String::new(), thumbnail: false, modified: 0, birth: None, icon: String::new() }
}

// A file row refuses the same chord the folder row above accepts.
fn file_row() -> crate::tui::model::Row {
    crate::tui::model::Row { name: "note.txt".into(), directory: false, size: 3, mode: 0o100644, link: String::new(), kind: String::new(), thumbnail: false, modified: 0, birth: None, icon: String::new() }
}

#[test]
fn ctrl_return_opens_only_a_folder_in_a_new_tab() {
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    model.rows.insert(0, folder_row());
    press(&mut model, &mut wire, &open_key());
    assert_eq!(model.tabs.len(), 2, "a folder row opens one tab");
    assert_eq!(model.tabs[1].path, PathBuf::from("/listing/sub"), "the new tab stands on that folder");
    drain(&mut wire);
    finish(wire, reader);
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    model.rows.insert(0, file_row());
    press(&mut model, &mut wire, &open_key());
    assert_eq!(model.tabs.len(), 1, "a file row opens no tab");
    assert_eq!(model.message, "Only a folder opens in a new tab", "a file row says only a folder does");
    finish(wire, reader);
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    press(&mut model, &mut wire, &open_key());
    assert_eq!(model.tabs.len(), 1, "no row opens no tab");
    assert_eq!(model.message, "Only a folder opens in a new tab", "no row says the same sentence");
    finish(wire, reader);
}

#[test]
fn a_tenth_tab_is_refused_in_both_adding_arms() {
    let cap = crate::uischema::MAX_LAST_TABS;
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    model.tabs = (0..cap).map(|_| crate::tui::model::Tab { path: PathBuf::from("/listing"), cursor: 0, back: Vec::new(), forward: Vec::new() }).collect();
    model.tab = cap - 1;
    model.rows.insert(0, folder_row());
    press(&mut model, &mut wire, &Key::character('t', ""));
    assert_eq!(model.tabs.len(), cap, "t on a full strip adds nothing");
    assert_eq!(model.message, "Nine tabs is the most", "t on a full strip says the cap");
    model.message.clear();
    press(&mut model, &mut wire, &open_key());
    assert_eq!(model.tabs.len(), cap, "ctrl-return on a full strip adds nothing");
    assert_eq!(model.message, "Nine tabs is the most", "ctrl-return on a full strip says the cap");
    finish(wire, reader);
}

#[test]
fn reorder_keys_move_the_current_tab_and_send_nothing() {
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    for _ in 0..2 {
        press(&mut model, &mut wire, &Key::character('t', ""));
        settle_open(&mut model, &mut wire);
    }
    assert_eq!(model.tabs.len(), 3, "two new tabs stand beside the first");
    assert_eq!(model.tab, 2, "a new tab lands current");
    model.tabs[0].cursor = 10;
    model.tabs[1].cursor = 20;
    model.tabs[2].cursor = 30;
    press(&mut model, &mut wire, &move_key("{"));
    assert_eq!(cursors(&model), vec![10, 30, 20], "{{ swaps the current tab left");
    assert_eq!(model.tab, 1, "and the current tab stays current");
    assert!(drain(&mut wire).is_empty(), "and a move lists nothing");
    press(&mut model, &mut wire, &move_key("}"));
    assert_eq!(cursors(&model), vec![10, 20, 30], "}} moves it back");
    assert_eq!(model.tab, 2, "still current");
    press(&mut model, &mut wire, &shift_page(true));
    assert_eq!(cursors(&model), vec![10, 30, 20], "ctrl-shift-pageup moves left too");
    press(&mut model, &mut wire, &shift_page(false));
    assert_eq!(cursors(&model), vec![10, 20, 30], "ctrl-shift-pagedown moves right too");
    press(&mut model, &mut wire, &move_key("}"));
    assert_eq!(cursors(&model), vec![10, 20, 30], "}} on the last tab is a no-op");
    assert_eq!(model.tab, 2, "and stays current");
    finish(wire, reader);
}

// Issue 390: a switch's list names every other tab's folder, so the backend keeps their settled sizes.
#[test]
fn a_tab_switch_names_the_other_tabs_folders_in_its_list() {
    let (mut wire, reader) = echo_wire();
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    press(&mut model, &mut wire, &Key::character('t', ""));
    settle_open(&mut model, &mut wire);
    model.tabs[0].path = PathBuf::from("/elsewhere");
    press(&mut model, &mut wire, &Key::character('1', ""));
    let sent = drain(&mut wire);
    let list = super::sort_tests::requests(&sent, "list");
    assert_eq!(list.len(), 1, "one switch lists once");
    assert_eq!(
        text(list[0], "path"),
        "/elsewhere",
        "the switch lists the first tab's folder"
    );
    let keep: Vec<&str> = list[0]
        .get("keep")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .filter_map(Json::as_str)
        .collect();
    assert_eq!(
        keep,
        vec!["/listing"],
        "and keeps the folder the second tab shows, never its own"
    );
    finish(wire, reader);
}
