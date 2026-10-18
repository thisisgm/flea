use super::*;
use crate::jsondoc;
use crate::tui::{input::Key, keymap::Map};

fn key(s: &str) -> Key {
    if !s.starts_with('<') { return Key::character(s.chars().next().unwrap(), ""); }
    let mut text = &s[1..s.len()-1];
    let mut mods = String::new();
    while text.len() > 2 && text.as_bytes()[1] == b'-' {
        mods.push_str(match &text[..1] { "C" => "ctrl", "S" => "shift", "A" => "alt", _ => panic!("unknown modifier") });
        text = &text[2..];
    }
    let name = match text { "Esc" => "Escape", "Enter" => "Return", "Space" => "Space", _ => text };
    if name.len() == 1 { Key::character(name.chars().next().unwrap(), &mods) } else { Key::named(name, &mods) }
}

#[test]
fn every_captured_action_has_a_real_binding_or_a_reasoned_unsupported_disposition() {
    let map = Map::load();
    let mut counts = std::collections::BTreeMap::new();
    for line in include_str!("parity.tsv").lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 8);
        *counts.entry(row[0]).or_insert(0) += 1;
        assert!(!row[7].is_empty());
        let sequence = jsondoc::parse(row[4]).unwrap();
        let keys: Vec<_> = sequence.as_array().unwrap().iter().map(|j| key(j.as_str().unwrap())).collect();
        if row[6] == "unsupported" {
            assert!(keys.is_empty());
            assert_eq!(refusal(&format!("unsupported:{}", row[7])), row[7]);
            continue;
        }
        assert!(!keys.is_empty());
        match row[3] {
            "listing" | "panel" => {
                let found = if keys.len() == 1 { map.in_context(&keys[0], "yazi", row[3]) }
                    else {
                        let prefix = map.action(&keys[0], "yazi");
                        map.sequence(prefix.strip_prefix("prefix:").unwrap(), &keys[1], "yazi", row[3])
                    };
                assert_eq!(found, row[5], "{} {}: {}", row[0], row[1], row[7]);
                assert!(crate::tui::keymap::supported(&found) || found.strip_prefix("tab").is_some_and(|s| s.parse::<usize>().is_ok()));
            }
            "editor" => {
                let mut editor = crate::tui::editor::Editor::new("path", "ab".into(), "/".into());
                if !matches!(row[5], "close" | "escape" | "close --submit") {
                    if row[5] == "backspace --under" { editor.update(&Key::named("Home", "")); }
                    editor.update(&keys[0]);
                    editor.update(&Key::character('x', ""));
                    let expected = match row[5] { "move -1" => "axb", "move bol" => "xab", "backspace" => "ax", "backspace --under" => "xb", _ => "abx" };
                    assert_eq!(editor.value, expected, "{} {}", row[0], row[1]);
                }
            }
            "confirm" | "completion" => assert!(matches!(row[5], "close" | "close --submit")),
            other => panic!("unverified context {other}"),
        }
    }
    assert_eq!(counts.values().sum::<usize>(), 254);
    assert_eq!(counts.keys().copied().collect::<Vec<_>>(), ["cmp", "confirm", "help", "input", "mgr", "pick", "spot", "tasks"]);
}
