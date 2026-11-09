use crate::jsondoc::{self, Json};
use super::{actions, input::Key, keymap::Map, model::Model, wire::{self, text}};
use std::path::PathBuf;

#[path = "action_regression_tests.rs"]
mod regressions;

pub(super) fn drain(wire: &mut wire::Wire) -> Vec<Json> {
    wire.send(vec![("c", wire::word("fence"))]).unwrap();
    let mut values = Vec::new();
    loop {
        let value = wire.events.recv_timeout(std::time::Duration::from_secs(10)).unwrap().unwrap();
        if text(&value, "c") == "fence" { return values; }
        values.push(value);
    }
}
fn requests<'a>(sent: &'a [Json], command: &str) -> Vec<&'a Json> {
    sent.iter().filter(|v| text(v, "c") == command).collect()
}

pub(super) fn listing(wire: &mut wire::Wire, preset: &str) -> Model {
    let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
    model.preset = preset.into();
    model.open(model.path.clone(), wire).unwrap();
    drain(wire);
    model.receive(jsondoc::parse(r#"{"t":"listed","n":2}"#).unwrap(), wire).unwrap();
    drain(wire);
    model.receive(jsondoc::parse(r#"{"t":"rows","start":0,"rows":[{"n":"folder","d":true},{"n":"file"}]}"#).unwrap(), wire).unwrap();
    drain(wire);
    model
}
fn press(m: &mut Model, w: &mut wire::Wire, key: Key) { actions::key(m, &key, &Map::load(), w).unwrap(); }

#[test]
fn action_pairs_reach_the_same_handlers_and_tab_focus_returns_to_listing() {
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        for key in [Key::named("Return", "ctrl"), Key::character('O', "")] {
            let (mut wire, reader) = wire::echo_wire();
            let mut m = listing(&mut wire, preset);
            press(&mut m, &mut wire, key);
            assert_eq!(m.tabs.len(), 2);
            assert_eq!(m.pending.as_deref(), Some(std::path::Path::new("/listing/folder")));
            assert_eq!(requests(&drain(&mut wire), "list").len(), 1);
            wire::finish(wire, reader);
        }
        let (mut wire, reader) = wire::echo_wire();
        let mut m = listing(&mut wire, preset);
        press(&mut m, &mut wire, Key::named("Tab", ""));
        assert!(m.preview_focus);
        press(&mut m, &mut wire, Key::named("Tab", ""));
        assert!(m.preview_focus, "text preview keeps its shipped Tab behavior");
        press(&mut m, &mut wire, Key::named("Tab", "ctrl"));
        assert!(!m.preview_focus);
        press(&mut m, &mut wire, Key::named("Tab", "ctrl"));
        assert!(m.preview_focus);
        press(&mut m, &mut wire, Key::named("Tab", "ctrl"));
        assert!(!m.preview_focus);
        for key in [Key::character('Y', "ctrl"), Key::character('Z', "ctrlshift")] {
            press(&mut m, &mut wire, key);
            assert_eq!(requests(&drain(&mut wire), "redo").len(), 1);
        }
        press(&mut m, &mut wire, Key::character('N', ""));
        assert_eq!(m.editor.as_ref().unwrap().kind, "mkdir");
        press(&mut m, &mut wire, Key::character('E', ""));
        assert!(m.edit_file.is_none(), "uppercase E is text inside an editor");
        press(&mut m, &mut wire, Key::named("Escape", ""));
        m.cursor = 1;
        press(&mut m, &mut wire, Key::character('E', "shift"));
        assert_eq!(m.edit_file.as_deref(), Some(std::path::Path::new("/listing/file")));
        m.edit_file = None;
        press(&mut m, &mut wire, Key::character('X', ""));
        assert!(m.deletion.is_none());
        press(&mut m, &mut wire, Key::named("Delete", "shift"));
        assert!(m.deletion.is_some());
        press(&mut m, &mut wire, Key::named("Escape", ""));
        assert!(m.deletion.is_none());
        wire::finish(wire, reader);
    }
}

#[test]
fn an_empty_folder_keeps_tab_in_the_listing_so_q_still_quits() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = Model::new(PathBuf::from("/empty"), &Json::Null);
    m.open(m.path.clone(), &mut wire).unwrap();
    drain(&mut wire);
    m.receive(
        jsondoc::parse(r#"{"t":"listed","n":0}"#).unwrap(),
        &mut wire,
    )
    .unwrap();
    drain(&mut wire);
    assert!(m.preview_visible);
    for key in [Key::named("Tab", ""), Key::named("Tab", "ctrl")] {
        press(&mut m, &mut wire, key);
        assert!(
            !m.preview_focus,
            "nothing to preview, so the listing keeps its keys"
        );
    }
    (m.columns, m.height) = (130, 27);
    let (left, middle, _) = super::render::panes(m.columns, m.preview_visible);
    let mut click = Key::named("", "");
    let (x, y) = (left + middle + 10, super::render::BODY_ROW + 3);
    click.pointer = Some(super::input::Pointer {
        button: 0,
        x,
        y,
        released: false,
        motion: false,
    });
    press(&mut m, &mut wire, click);
    assert!(
        !m.preview_focus,
        "a press on the empty preview leaves the keys with the listing"
    );
    m.preview_focus = true;
    press(&mut m, &mut wire, Key::character('q', ""));
    assert!(
        m.quit && !m.preview_focus,
        "a focus left on the empty preview by any other path hands q back"
    );
    wire::finish(wire, reader);
}

#[test]
fn a_folder_that_empties_under_the_preview_hands_q_back_to_the_listing() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = listing(&mut wire, "default");
    m.receive(
        jsondoc::parse(r#"{"t":"listed","n":5}"#).unwrap(),
        &mut wire,
    )
    .unwrap();
    drain(&mut wire);
    m.cursor = 4;
    press(&mut m, &mut wire, Key::named("Tab", ""));
    assert!(
        m.preview_focus,
        "a row still being fetched keeps Tab's preview stop"
    );
    m.receive(
        jsondoc::parse(r#"{"t":"listed","n":0}"#).unwrap(),
        &mut wire,
    )
    .unwrap();
    drain(&mut wire);
    assert!(
        !m.preview_focus,
        "the re-list itself stops drawing the empty preview as focused"
    );
    m.receive(
        jsondoc::parse(r#"{"t":"listed","n":5}"#).unwrap(),
        &mut wire,
    )
    .unwrap();
    drain(&mut wire);
    press(&mut m, &mut wire, Key::named("Tab", ""));
    assert!(m.preview_focus);
    m.search = "Search: none".into();
    m.receive(
        jsondoc::parse(r#"{"t":"searched","n":0,"scanned":5}"#).unwrap(),
        &mut wire,
    )
    .unwrap();
    drain(&mut wire);
    assert!(
        !m.preview_focus,
        "a search with no results stops drawing the empty preview as focused"
    );
    press(&mut m, &mut wire, Key::character('q', ""));
    assert!(m.quit);
    wire::finish(wire, reader);
}

#[test]
fn mac_move_and_duplicate_have_plain_keys_and_windows_ctrl_d_still_trashes() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = listing(&mut wire, "mac");
    m.clipboard = vec!["/source/file".into()];
    press(&mut m, &mut wire, Key::character('M', ""));
    // Paste asks the destination for collisions first; with none the move goes ahead.
    let id = wire::count(requests(&drain(&mut wire), "collisions")[0], "id");
    m.receive(jsondoc::parse(&format!(r#"{{"t":"collisions","id":{id},"total":0,"names":[],"mergeable":false}}"#)).unwrap(), &mut wire).unwrap();
    let sent = drain(&mut wire);
    assert_eq!(text(requests(&sent, "transfer")[0], "op"), "move");
    press(&mut m, &mut wire, Key::character('U', ""));
    assert_eq!(requests(&drain(&mut wire), "duplicate").len(), 1);
    m.preset = "windows".into();
    press(&mut m, &mut wire, Key::character('D', "ctrl"));
    assert_eq!(requests(&drain(&mut wire), "trash").len(), 1);
    wire::finish(wire, reader);
}

#[test]
fn unsupported_actions_say_why_and_sequences_do_not_leak_into_another_context() {
    check_unsupported_actions_and_sequences(&Map::load());
}

fn check_unsupported_actions_and_sequences(map: &Map) {
    let press = |m: &mut Model, w: &mut wire::Wire, key: Key| actions::key(m, &key, map, w).unwrap();
    let (mut wire, reader) = wire::echo_wire();
    let mut m = listing(&mut wire, "default");
    press(&mut m, &mut wire, Key::named("F5", ""));
    assert_eq!(requests(&drain(&mut wire), "list").len(), 1);
    m.pending = None;
    press(&mut m, &mut wire, Key::character(',', ""));
    assert_eq!(m.error, "Settings are available in the Flea window");
    press(&mut m, &mut wire, Key::named("Escape", ""));
    m.preset = "yazi".into();
    m.total = 3;
    m.receive(jsondoc::parse(r#"{"t":"rows","start":2,"rows":[{"n":"last-file"}]}"#).unwrap(), &mut wire).unwrap();
    drain(&mut wire);
    m.cursor = 2;
    press(&mut m, &mut wire, Key::character('g', ""));
    assert_eq!(m.key_arm, "prefix:g");
    press(&mut m, &mut wire, Key::character('g', ""));
    assert_eq!(m.cursor, 0);
    assert!(m.key_arm.is_empty());
    m.cursor = 1;
    press(&mut m, &mut wire, Key::character('g', ""));
    m.sheet = true;
    press(&mut m, &mut wire, Key::character('g', ""));
    assert_eq!(m.cursor, 1);
    assert!(m.key_arm.is_empty());
    assert!(m.sheet);
    press(&mut m, &mut wire, Key::named("Escape", ""));
    press(&mut m, &mut wire, Key::character('g', ""));
    assert_eq!(m.key_arm, "prefix:g");
    press(&mut m, &mut wire, Key::named("Escape", ""));
    assert_eq!(m.cursor, 1);
    assert!(m.key_arm.is_empty());
    press(&mut m, &mut wire, Key::character(';', ""));
    assert!(m.error.contains("Shell prompts are unavailable"));
    press(&mut m, &mut wire, Key::character('g', ""));
    press(&mut m, &mut wire, Key::named("Escape", ""));
    assert_eq!(m.key_arm, "");
    wire::finish(wire, reader);
}

#[test]
fn sequence_checker_rejects_inert_navigation() {
    check_unsupported_actions_and_sequences(&Map::load());
    let text = include_str!("../../keys.toml");
    for action in ["clearSelection", "cursorUp"] {
        let wrong = text.replace("key = \"g\"\naction = \"cursorFirst\"", &format!("key = \"g\"\naction = \"{action}\""));
        assert_ne!(wrong, text);
        let result = std::panic::catch_unwind(|| check_unsupported_actions_and_sequences(&Map::parse(&wrong)));
        assert!(result.is_err(), "the sequence check accepted {action} instead of first-row navigation");
    }
}

// A symlink the backend resolved to a folder (d false, lstat mode a link, the folder icon) browses in like a folder.
#[test]
fn enter_right_and_l_on_a_link_to_a_folder_browse_into_it() {
    let link = r#"{"t":"rows","start":0,"rows":[{"n":"todir","d":false,"p":41471,"i":"folder","l":"../one"},{"n":"tofile","d":false,"p":41471,"i":"text-x-generic","l":"note.txt"}]}"#;
    for (preset, key) in [("default", Key::named("Return", "")), ("default", Key::named("Right", "")), ("vim", Key::character('l', "")), ("yazi", Key::character('l', ""))] {
        let (mut wire, reader) = wire::echo_wire();
        let mut m = listing(&mut wire, preset);
        m.receive(jsondoc::parse(link).unwrap(), &mut wire).unwrap();
        drain(&mut wire);
        m.cursor = 0;
        assert!(m.rows[&0].folder() && !m.rows[&1].folder(), "only the link to a folder is one");
        actions::key(&mut m, &key, &Map::load(), &mut wire).unwrap();
        assert_eq!(m.pending.as_deref(), Some(std::path::Path::new("/listing/todir")), "{preset} {}: the link path is listed", key.name);
        assert_eq!(requests(&drain(&mut wire), "list").len(), 1);
        assert!(m.error.is_empty(), "{preset}: {}", m.error);
        wire::finish(wire, reader);
    }
    // Its preview is a folder's: the children peek, never a file read that fails.
    let (mut wire, reader) = wire::echo_wire();
    let mut m = listing(&mut wire, "default");
    m.receive(jsondoc::parse(link).unwrap(), &mut wire).unwrap();
    drain(&mut wire);
    m.cursor = 0;
    m.preview_visible = true;
    super::preview::load(&mut m, true);
    super::preview::request(&mut m, &mut wire).unwrap();
    let sent = drain(&mut wire);
    assert_eq!(requests(&sent, "peek").len(), 1, "the link's preview peeks its folder");
    assert!(requests(&sent, "meta").is_empty());
    assert_eq!(super::pane_state::kind(&m.rows[&0]), super::pane_state::Kind::Folder);
    // Its facts say Folder and link, as the window's size column does, never the link's own bytes.
    let facts = super::pane::preview(&m, 1, 60, 20, &super::theme::Theme::with_depth("", super::theme::Depth::Mono));
    assert!(facts.contains("Folder · link") && !facts.contains(" B"), "{facts:?}");
    wire::finish(wire, reader);
}
