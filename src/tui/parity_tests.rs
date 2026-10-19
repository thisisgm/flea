use crate::jsondoc::{self, Json};
use crate::tui::{input::Key, keymap::Map};

#[path = "parity_action_tests.rs"]
mod dispatch;
#[path = "parity_regression_tests.rs"]
mod regressions;

// Sample input: "g", "<C-f>", "<S-PageUp>" or "<Enter>".
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

// Sample input: "<Esc>" or ["g", "g"] as JSON.
fn sequence(text: &str) -> Vec<Key> {
    match jsondoc::parse(text).unwrap() {
        Json::Str(text) => vec![key(&text)],
        Json::Arr(values) => values.iter().map(|value| key(value.as_str().unwrap())).collect(),
        _ => panic!("a key sequence must be a JSON string or array: {text}"),
    }
}

fn resolved(map: &Map, keys: &[Key], context: &str) -> String {
    match keys {
        [only] => map.in_context(only, "yazi", context),
        [first, second] => {
            let action = map.in_context(first, "yazi", context);
            let prefix = action.strip_prefix("prefix:").expect("a sequence starts with a prefix binding");
            map.sequence(prefix, second, "yazi", context)
        }
        _ => panic!("unsupported key sequence: {keys:?}"),
    }
}

#[test]
fn every_captured_action_has_a_real_binding_or_a_reasoned_unsupported_disposition() {
    check_rows(include_str!("parity.tsv"), &Map::load());
}

// Sample input: mgr\t"q"\t"quit"\tlisting\t["q"]\tquit\tbound\tThe equivalent action uses the resolved Flea handler.
fn check_rows(text: &str, map: &Map) {
    let mut counts = std::collections::BTreeMap::new();
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 8);
        *counts.entry(row[0]).or_insert(0) += 1;
        assert!(!row[7].is_empty());
        let upstream = sequence(row[1]);
        let keys = sequence(row[4]);
        match row[6] {
            "unsupported" => {
                assert!(keys.is_empty());
                dispatch::unsupported(&row, &upstream, map);
                continue;
            }
            "bound" => assert_eq!(upstream, keys, "{} {} claims the original key", row[0], row[1]),
            "different" => assert_ne!(upstream, keys, "{} {} claims a different key", row[0], row[1]),
            other => panic!("unknown parity disposition {other}"),
        }
        assert!(!keys.is_empty());
        match row[3] {
            "listing" | "panel" => {
                let found = resolved(map, &keys, row[3]);
                assert_eq!(found, row[5], "{} {}: {}", row[0], row[1], row[7]);
                assert!(crate::tui::keymap::supported(&found) || found.strip_prefix("tab").is_some_and(|s| s.parse::<usize>().is_ok()));
            }
            "editor" => dispatch::editor(&keys, row[5], map),
            "confirm" | "completion" => dispatch::dialog(&keys, row[3], row[5], map),
            other => panic!("unverified context {other}"),
        }
    }
    assert_eq!(counts.values().sum::<usize>(), 254);
    assert_eq!(counts.keys().copied().collect::<Vec<_>>(), ["cmp", "confirm", "help", "input", "mgr", "pick", "spot", "tasks"]);
}
