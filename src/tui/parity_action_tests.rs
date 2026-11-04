use super::{resolved, Key, Map};
use crate::tui::{action_key_tests::{drain, listing}, actions, editor::Editor, model::{Deletion, Model}, wire::{self, text}};

fn press(model: &mut Model, keys: &[Key], map: &Map, wire: &mut wire::Wire) {
    for key in keys { actions::key(model, key, map, wire).unwrap(); }
}

pub(super) fn editor(keys: &[Key], action: &str, map: &Map) {
    if matches!(action, "backward lean" | "forward lean --end-of-word" | "kill bol" | "kill eol" | "kill backward" | "kill forward") {
        return crate::tui::parity17_tests::editor_action(keys, action);
    }
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
    assert!(matches!(action, "close" | "close --submit" | "scrollUp" | "scrollDown"));
    if context == "confirm" {
        confirmation(&mut model);
        assert!(!model.deletion.as_ref().unwrap().destructive);
        if matches!(action, "scrollUp" | "scrollDown") {
            model.height = 6;
            model.columns = 16;
            model.deletion.as_mut().unwrap().scroll = 1;
            model.deletion.as_mut().unwrap().count = 1000;
            press(&mut model, keys, map, &mut wire);
            assert_eq!(model.deletion.as_ref().unwrap().scroll, if action == "scrollUp" { 0 } else { 2 });
            assert!(!model.deletion.as_ref().unwrap().destructive);
            press(&mut model, &[Key::named("Tab", "")], map, &mut wire);
            assert!(model.deletion.as_ref().unwrap().destructive);
            // Back to the middle, so the press with Delete focused takes a real scroll step either way.
            model.deletion.as_mut().unwrap().scroll = 1;
            press(&mut model, keys, map, &mut wire);
            assert_eq!(model.deletion.as_ref().unwrap().scroll, if action == "scrollUp" { 0 } else { 2 });
            assert!(model.deletion.as_ref().unwrap().destructive, "scrolling moved focus off Delete: {keys:?}");
            assert!(drain(&mut wire).is_empty());
            wire::finish(wire, reader);
            return;
        }
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
    assert_eq!((model.cursor, model.path), (0, std::path::PathBuf::from("/listing")));
    assert!(model.selected.is_empty() && model.clipboard.is_empty() && model.launches.is_empty());
    assert!(model.editor.is_none() && model.edit_file.is_none() && !model.quit);
    assert!(drain(&mut wire).is_empty());
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
            super::candidate::open_with(keys, "ignored", false, map);
        }
        "input" | "cmp" | "panel" | "confirm" => inactive_action(row, keys, map),
        other => panic!("unverified unsupported context {other}"),
    }
}

fn manager_refusal(row: &[&str], keys: &[Key], map: &Map) {
    let found = resolved(map, keys, "listing");
    if let Some(reason) = found.strip_prefix("unsupported:") {
        // The parity reason is the keymap's refusal, except rows whose reason spells out the route the refusal names.
        match row[8] {
            "yazi-034" => assert!(reason.contains("use Left") && row[7].contains("then Left seeks"), "{reason}"),
            "yazi-035" => assert!(reason.contains("use Right") && row[7].contains("then Right seeks"), "{reason}"),
            "yazi-063" => assert_eq!(reason, "detail:size"),
            "yazi-064" => assert_eq!(reason, "detail:mode"),
            "yazi-066" => assert_eq!(reason, "detail:modified"),
            "yazi-067" => assert_eq!(reason, "detail:owner"),
            "yazi-065" | "yazi-068" => assert_eq!(reason, "Specific detail modes are unavailable in this terminal build"),
            _ => assert_eq!(reason, row[7], "{} {}", row[0], row[1]),
        }
        let expected = super::super::refusal_text(&found).render(&crate::i18n::test_catalogs::english());
        refusal(keys, &expected, map);
        if let Some(field) = reason.strip_prefix("detail:") { super::listing_checks::detail_route(field, map); }
        if reason == "Specific detail modes are unavailable in this terminal build" { super::listing_checks::listing_action(&[Key::character('I', "")], "detailNext", "yazi", map); }
    } else {
        let (action, alternate) = match row[8] {
            "yazi-032" => ("toggleSelect", vec![Key::character('v', "")]),
            "yazi-033" => ("clearSelection", vec![Key::character('V', "")]),
            "yazi-053" => ("openAgent", vec![Key::character('a', "")]),
            "yazi-076" => ("filter", vec![Key::character('f', "")]),
            "yazi-079" => ("newFolder", vec![Key::character('N', "")]),
            "yazi-080" => ("sortDate", keys.to_vec()), "yazi-081" => ("sortDateReverse", keys.to_vec()),
            "yazi-090" => ("sortSize", keys.to_vec()), "yazi-091" => ("sortSizeReverse", keys.to_vec()),
            "yazi-086" => ("sortName", keys.to_vec()), "yazi-087" => ("sortNameReverse", keys.to_vec()),
            "yazi-085" => ("", keys.to_vec()),
            other => panic!("unsupported action lost its refusal: {other}: {found}"),
        };
        assert_eq!(found, action, "the intentional alternative changed: {}", row[1]);
        if row[8] == "yazi-032" { super::listing_checks::listing_action(&[Key::named("Down", "shift")], "extendDown", "yazi", map); }
        if action.is_empty() {
            let reason = crate::i18n::test_catalogs::english().tr("tui.actions.no_action_for_that_key_sequence");
            refusal(&alternate, &reason, map);
        } else {
            super::listing_checks::listing_action(&alternate, if action == "openAgent" { "newFile" } else { action }, "yazi", map);
        }
    }
}

fn inactive_action(row: &[&str], keys: &[Key], map: &Map) {
    let (mut wire, reader) = wire::echo_wire();
    let mut model = listing(&mut wire, "yazi");
    match row[3] {
        "input" | "cmp" => {
            let kind = if row[3] == "input" { "filter" } else { "path" };
            let mut editor = Editor::new(kind, "abc".into(), model.path.clone());
            if row[8] == "yazi-182" {
                editor.update(&Key::named("Home", ""));
                editor.update(&Key::named("Right", ""));
            }
            model.editor = Some(editor);
            press(&mut model, keys, map, &mut wire);
            if row[3] == "cmp" && keys == [Key::named("Return", "")] {
                assert!(model.editor.is_none());
                assert!(model.pending.is_some());
            } else {
                let typed: String = keys.iter().filter(|key| matches!(key.mods.as_str(), "" | "shift")).map(|key| key.text.as_str()).collect();
                let expected = if row[8] == "yazi-165" { "abc".into() } else { format!("abc{typed}") };
                assert_eq!(model.editor.as_ref().unwrap().value, expected, "insert-only field: {}", row[1]);
                if matches!(row[8], "yazi-165" | "yazi-181" | "yazi-182") {
                    press(&mut model, &[Key::character('x', "")], map, &mut wire);
                    assert_eq!(model.editor.as_ref().unwrap().value, if row[8] == "yazi-165" { "x" } else { "abxc" });
                }
            }
            assert!(!model.sheet && !model.menu);
            assert_eq!(model.cursor, 0);
            assert!(model.selected.is_empty() && model.clipboard.is_empty() && model.launches.is_empty() && model.edit_file.is_none());
            assert!(drain(&mut wire).iter().all(|v| matches!(text(v, "c"), "filter" | "list" | "search")));
        }
        "panel" => {
            model.sheet = true;
            press(&mut model, keys, map, &mut wire);
            assert!(model.clipboard.is_empty());
            assert!(!model.preview_focus);
            assert!(model.editor.is_none());
            assert_eq!(model.path, std::path::Path::new("/listing"));
            assert_eq!(model.sheet, row[1] != "\"<Enter>\"");
            assert!(model.launches.is_empty() && model.edit_file.is_none() && model.selected.is_empty());
            assert!(drain(&mut wire).is_empty());
        }
        "confirm" => {
            confirmation(&mut model);
            press(&mut model, keys, map, &mut wire);
            assert!(!model.deletion.as_ref().unwrap().destructive);
            press(&mut model, &[Key::named("Tab", "")], map, &mut wire);
            assert!(model.deletion.as_ref().unwrap().destructive);
            press(&mut model, keys, map, &mut wire);
            assert!(model.deletion.as_ref().unwrap().destructive, "{} moved focus off Delete", row[1]);
            assert!(!drain(&mut wire).iter().any(|value| text(value, "op") == "delete"));
        }
        _ => unreachable!(),
    }
    wire::finish(wire, reader);
}
