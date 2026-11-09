// Issue 133 through the real key path: on a GVFS mount every preset's trash keys reach no backend and say why.
use super::sort_tests::{drain, press, requests};
use super::*;
use crate::tui::input::Key;
use crate::tui::keymap::{Hints, Map};
use crate::tui::model::Model;
use std::path::PathBuf;

// The box's own SMB share and a phone, as gvfs names their FUSE folders; gio trash refuses both.
const SHARE: &str = "/run/user/1000/gvfs/smb-share:server=192.168.21.25,share=data";
const PHONE: &str = "/run/user/1000/gvfs/mtp:host=SAMSUNG_RQGL705T0NR/DCIM";
const REFUSAL: &str = "This location has no Trash; Shift+Delete deletes permanently";

fn key(name: &str, text: &str, mods: &str) -> Key {
    Key { name: name.into(), text: text.into(), mods: mods.into(), pointer: None }
}

// Every shipped way to trash, per preset, checked against the shipped map: dd, Delete, vim's D,
// Windows' Ctrl+D and the Mac's Ctrl+Delete.
fn trash_keys() -> Vec<(&'static str, Key)> {
    let keys = vec![
        ("default", key("d", "d", "")),
        ("default", key("Delete", "", "")),
        ("vim", key("D", "D", "")),
        ("windows", key("D", "", "ctrl")),
        ("mac", key("Delete", "", "ctrl")),
    ];
    for (preset, pressed) in &keys {
        let action = Map::load().action(pressed, preset);
        assert!(matches!(action.as_str(), "trash" | "trashArm"), "{preset} {} is a shipped trash key, not {action:?}", pressed.name);
    }
    keys
}

// One file listed under dir through the real receive path, with the cursor on it.
fn listing(dir: &str, preset: &str, wire: &mut Wire) -> Model {
    let mut model = Model::new(PathBuf::from(dir), &Json::Null);
    model.preset = preset.into();
    model.hints = Hints::new(&Map::load(), preset);
    model.open(model.path.clone(), wire).unwrap();
    drain(wire);
    let listed = format!(r#"{{"t":"listed","n":1,"read":0.0,"sort":0.0,"v":1,"path":"{dir}"}}"#);
    model.receive(jsondoc::parse(&listed).unwrap(), wire).unwrap();
    drain(wire);
    model.receive(jsondoc::parse(r#"{"t":"rows","start":0,"rows":[{"n":"photo.jpg"}]}"#).unwrap(), wire).unwrap();
    drain(wire);
    model
}

#[test]
fn trash_keys_on_a_share_or_a_phone_send_nothing_and_say_why() {
    for dir in [SHARE, PHONE] {
        for (preset, pressed) in trash_keys() {
            let (mut wire, reader) = echo_wire();
            let mut model = listing(dir, preset, &mut wire);
            // Twice, so dd's second press is covered as well as a single-press key pressed again.
            for _ in 0..2 {
                press(&mut model, &mut wire, &pressed);
                assert!(requests(&drain(&mut wire), "trash").is_empty(), "{preset} {} in {dir} asks for no trash", pressed.name);
                assert_eq!(model.key_arm, "", "{preset} {} in {dir} arms nothing", pressed.name);
            }
            assert_eq!(model.error, REFUSAL, "{preset}: the status line names the failure and the way out");
            // The way out it names is real: Shift+Delete still asks to delete permanently here.
            press(&mut model, &mut wire, &key("Delete", "", "shift"));
            let sent = drain(&mut wire);
            assert_eq!(requests(&sent, "menuaction").len(), 1, "{preset} Shift+Delete in {dir} starts permanent deletion");
            finish(wire, reader);
        }
    }
}

#[test]
fn trash_keys_in_a_local_folder_still_trash() {
    for (preset, pressed) in trash_keys() {
        let (mut wire, reader) = echo_wire();
        let mut model = listing("/listing", preset, &mut wire);
        press(&mut model, &mut wire, &pressed);
        press(&mut model, &mut wire, &pressed);
        assert!(!requests(&drain(&mut wire), "trash").is_empty(), "{preset} {} trashes a local file", pressed.name);
        assert_eq!(model.error, "", "{preset}: nothing is refused");
        finish(wire, reader);
    }
}

// The keys the prompt and the refusal name come from the hints, so a rebound or missing key changes the sentence.
#[test]
fn the_arm_prompt_and_the_refusal_name_the_keys_the_hints_hold() {
    let rebound = Map::parse(r##"
[[preset]]
name = "default"
mods = "ctrl"
key = "K"
action = "deletePermanently"
context = "listing"
[[code]]
key = "Backspace"
action = "trash"
"##);
    let (mut wire, reader) = echo_wire();
    let mut model = listing("/listing", "default", &mut wire);
    press(&mut model, &mut wire, &key("d", "d", ""));
    assert_eq!(model.message, "Press d again to move it to Trash, or press Delete on its own.", "the shipped keys");
    model.hints = Hints::new(&rebound, "default");
    model.key_arm.clear();
    press(&mut model, &mut wire, &key("d", "d", ""));
    assert_eq!(model.message, "Press d again to move it to Trash, or press Backspace on its own.", "a rebound trash key");
    model.hints.trash.clear();
    model.key_arm.clear();
    press(&mut model, &mut wire, &key("d", "d", ""));
    assert_eq!(model.message, "Press d again to move it to Trash.", "no key left to name");
    finish(wire, reader);
    let (mut wire, reader) = echo_wire();
    let mut model = listing(SHARE, "default", &mut wire);
    model.hints = Hints::new(&rebound, "default");
    press(&mut model, &mut wire, &key("Delete", "", ""));
    assert_eq!(model.error, "This location has no Trash; Ctrl+K deletes permanently", "a rebound permanent-delete key");
    model.dismiss_error();
    model.hints.delete_permanently.clear();
    press(&mut model, &mut wire, &key("Delete", "", ""));
    assert_eq!(model.error, "This location has no Trash", "no key left to name");
    finish(wire, reader);
}

// An empty folder has nothing to trash: d arms no prompt and no trash key sends anything.
#[test]
fn trash_keys_in_an_empty_folder_arm_nothing_and_say_nothing() {
    for (preset, pressed) in trash_keys() {
        let (mut wire, reader) = echo_wire();
        let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
        model.preset = preset.into();
        model.hints = Hints::new(&Map::load(), preset);
        model.open(model.path.clone(), &mut wire).unwrap();
        drain(&mut wire);
        model.receive(jsondoc::parse(r#"{"t":"listed","n":0,"read":0.0,"sort":0.0,"v":1,"path":"/listing"}"#).unwrap(), &mut wire).unwrap();
        drain(&mut wire);
        assert_eq!(model.total, 0);
        for _ in 0..2 {
            press(&mut model, &mut wire, &pressed);
            assert!(requests(&drain(&mut wire), "trash").is_empty(), "{preset} {} sends no trash", pressed.name);
            assert_eq!(model.key_arm, "", "{preset} {} arms nothing", pressed.name);
            assert_eq!(model.message, "", "{preset} {} leaves no prompt", pressed.name);
        }
        finish(wire, reader);
    }
}
