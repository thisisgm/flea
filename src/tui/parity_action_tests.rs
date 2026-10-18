use super::{resolved, Key, Map};
use crate::tui::{action_key_tests::{drain, listing}, actions, editor::Editor, model::{Deletion, Model}, wire::{self, text}};

fn press(model: &mut Model, keys: &[Key], map: &Map, wire: &mut wire::Wire) {
    for key in keys { actions::key(model, key, map, wire).unwrap(); }
}

pub(super) fn editor(keys: &[Key], action: &str, map: &Map) {
    let (mut wire, reader) = wire::echo_wire();
    let mut model = listing(&mut wire, "yazi");
    let mut editor = Editor::new("filter", "abc".into(), model.path.clone());
    match action {
        "move 1" | "backspace --under" => editor.update(&Key::named("Home", "")),
        "move eol" => {
            editor.update(&Key::named("Home", ""));
            editor.update(&Key::named("Right", ""));
        }
        _ => {}
    }
    model.editor = Some(editor);
    press(&mut model, keys, map, &mut wire);
    match action {
        "close" | "escape" | "close --submit" => {
            assert!(model.editor.is_none(), "{action} did not close the field: {keys:?}");
            assert_eq!(model.filter, if action == "close --submit" { "abc" } else { "" });
        }
        _ => {
            press(&mut model, &[Key::character('x', "")], map, &mut wire);
            let expected = match action {
                "move -1" => "abxc", "move 1" => "axbc", "move bol" => "xabc", "move eol" => "abcx",
                "backspace" => "abx", "backspace --under" => "xbc", _ => panic!("unverified editor action {action}"),
            };
            assert_eq!(model.editor.as_ref().unwrap().value, expected, "{action}: {keys:?}");
        }
    }
    wire::finish(wire, reader);
}

fn confirmation(model: &mut Model) {
    model.deletion = Some(Deletion { token: 7, count: 1, ..Deletion::default() });
    model.menu_action = "delete".into();
}

pub(super) fn dialog(keys: &[Key], context: &str, action: &str, map: &Map) {
    let (mut wire, reader) = wire::echo_wire();
    let mut model = listing(&mut wire, "yazi");
    assert!(matches!(action, "close" | "close --submit"));
    if context == "confirm" {
        confirmation(&mut model);
        assert!(!model.deletion.as_ref().unwrap().destructive);
        press(&mut model, keys, map, &mut wire);
        assert!(model.deletion.is_none(), "confirmation did not close: {keys:?}");
        let sent = drain(&mut wire);
        let operations: Vec<_> = sent.iter().filter(|value| text(value, "c") == "menuaction").map(|value| text(value, "op")).collect();
        assert_eq!(operations, [if action == "close --submit" { "delete" } else { "close" }]);
        if action == "close --submit" {
            confirmation(&mut model);
            press(&mut model, &[Key::named("Return", "")], map, &mut wire);
            let sent = drain(&mut wire);
            assert!(sent.iter().any(|value| text(value, "op") == "close"));
            assert!(!sent.iter().any(|value| text(value, "op") == "delete"), "Enter must keep the safe Cancel default");
        }
    } else {
        model.editor = Some(Editor::new("path", "/listing/f".into(), model.path.clone()));
        model.completion = "/listing/folder".into();
        press(&mut model, keys, map, &mut wire);
        if action == "close" { assert!(model.editor.is_none(), "completion field did not close: {keys:?}"); }
        else { assert_eq!(model.editor.as_ref().unwrap().value, "/listing/folder", "completion was not accepted: {keys:?}"); }
    }
    wire::finish(wire, reader);
}

fn refusal(keys: &[Key], reason: &str, map: &Map) {
    let (mut wire, reader) = wire::echo_wire();
    let mut model = listing(&mut wire, "yazi");
    press(&mut model, keys, map, &mut wire);
    assert_eq!(model.error, reason, "{keys:?}");
    wire::finish(wire, reader);
}

pub(super) fn unsupported(row: &[&str], keys: &[Key], map: &Map) {
    match row[3] {
        "mgr" => manager_refusal(row, keys, map),
        "tasks" => {
            assert!(keys.iter().all(|key| map.in_context(key, "yazi", "tasks").is_empty()));
            refusal(&[Key::character('w', "")], "A task list is unavailable; running operations report in the status row", map);
        }
        "pick" => {
            assert!(keys.iter().all(|key| map.in_context(key, "yazi", "pick").is_empty()));
        }
        "input" | "cmp" | "panel" | "confirm" => inactive_action(row, keys, map),
        other => panic!("unverified unsupported context {other}"),
    }
}

fn manager_refusal(row: &[&str], keys: &[Key], map: &Map) {
    let found = resolved(map, keys, "listing");
    if let Some(reason) = found.strip_prefix("unsupported:") {
        assert_eq!(reason, row[7], "{} {}", row[0], row[1]);
        refusal(keys, reason, map);
    } else {
        let expected = match row[1] {
            "\"v\"" => "toggleSelect", "\"V\"" => "clearSelection", "\"O\"" => "openTab", "\"A\"" => "openAgent",
            "\"/\"" => "filter", "\"N\"" => "newFolder",
            "[\",\", \"m\"]" => "sortDate", "[\",\", \"M\"]" => "sortDateReverse",
            "[\",\", \"a\"]" => "sortName", "[\",\", \"A\"]" => "sortNameReverse",
            "[\",\", \"s\"]" => "sortSize", "[\",\", \"S\"]" => "sortSizeReverse",
            "[\",\", \"E\"]" | "[\",\", \"N\"]" => "",
            other => panic!("unsupported action lost its refusal: {other}: {found}"),
        };
        assert_eq!(found, expected, "the intentional alternative changed: {}", row[1]);
    }
}

fn inactive_action(row: &[&str], keys: &[Key], map: &Map) {
    let (mut wire, reader) = wire::echo_wire();
    let mut model = listing(&mut wire, "yazi");
    match row[3] {
        "input" | "cmp" => {
            let kind = if row[3] == "input" { "filter" } else { "path" };
            model.editor = Some(Editor::new(kind, "abc".into(), model.path.clone()));
            press(&mut model, keys, map, &mut wire);
            if row[3] == "cmp" && keys == [Key::named("Return", "")] {
                assert!(model.editor.is_none());
                assert!(model.pending.is_some());
            } else {
                let typed: String = keys.iter().filter(|key| matches!(key.mods.as_str(), "" | "shift")).map(|key| key.text.as_str()).collect();
                assert_eq!(model.editor.as_ref().unwrap().value, format!("abc{typed}"), "insert-only field: {}", row[1]);
            }
            assert!(!model.sheet && !model.menu);
        }
        "panel" => {
            model.sheet = true;
            press(&mut model, keys, map, &mut wire);
            assert!(model.clipboard.is_empty());
            assert!(!model.preview_focus);
            assert!(model.editor.is_none());
            assert_eq!(model.path, std::path::Path::new("/listing"));
            assert_eq!(model.sheet, row[1] != "\"<Enter>\"");
        }
        "confirm" => {
            confirmation(&mut model);
            press(&mut model, keys, map, &mut wire);
            assert!(!model.deletion.as_ref().unwrap().destructive);
            assert!(!drain(&mut wire).iter().any(|value| text(value, "op") == "delete"));
        }
        _ => unreachable!(),
    }
    wire::finish(wire, reader);
}
