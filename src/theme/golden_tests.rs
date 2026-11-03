use super::{Theme, Values};
use std::collections::{BTreeMap, BTreeSet};

type Cases = BTreeMap<(String, u32), BTreeMap<String, String>>;

pub(super) fn cases() -> Cases {
    let mut out = Cases::new();
    for (i, line) in include_str!("../../tests/fixtures/theme-golden.tsv")
        .lines()
        .enumerate()
    {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 4, "golden line {}", i + 1);
        let key = (fields[0].to_owned(), fields[1].parse().expect("size stop"));
        let prior = out
            .entry(key)
            .or_default()
            .insert(fields[2].into(), fields[3].into());
        assert!(prior.is_none(), "duplicate golden line {}", i + 1);
    }
    out
}

pub(super) fn resolve(rows: &BTreeMap<String, String>) -> Theme {
    let mut colors = String::new();
    let mut shell = String::new();
    let mut user = String::new();
    for (key, value) in rows {
        if let Some(key) = key.strip_prefix("input.colors.") {
            let key = unnumber(key);
            colors.push_str(&format!("{key} = \"{value}\"\n"));
        } else {
            let target = if let Some(key) = key.strip_prefix("input.shell.") {
                Some((&mut shell, key))
            } else {
                key.strip_prefix("input.user.").map(|key| (&mut user, key))
            };
            if let Some((target, key)) = target {
                let key = unnumber(key);
                let (section, key) = key.split_once('.').expect("shell section");
                target.push_str(&format!("[{section}]\n{key} = \"{value}\"\n"));
            }
        }
    }
    Theme::resolve(&colors, &shell, &user, None, "monospace")
}

fn unnumber(key: &str) -> &str {
    match key.split_once('.') {
        Some((head, rest)) if head.len() == 6 && head.bytes().all(|b| b.is_ascii_digit()) => rest,
        _ => key,
    }
}

#[test]
fn theme_golden_all_palettes_and_stops() {
    let cases = cases();
    let names: BTreeSet<_> = cases.keys().map(|(name, _)| name).collect();
    assert!(names.len() >= 23, "stock palette coverage shrank");
    for name in names {
        for stop in [12, 14] {
            assert!(
                cases.contains_key(&(name.clone(), stop)),
                "missing {name}/{stop}"
            );
        }
    }
    for ((name, stop), rows) in &cases {
        let theme = resolve(rows);
        assert_eq!(
            theme.font.base_size, *stop,
            "{name}/{stop} must exercise its stop"
        );
        let actual = theme.tokens();
        let expected: Values = rows
            .iter()
            .filter(|(k, _)| !k.starts_with("input."))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(actual.len(), expected.len(), "{name}/{stop} token coverage");
        for (token, value) in actual {
            assert_eq!(expected.get(&token), Some(&value), "{name}/{stop}/{token}");
        }
    }
}

// The provisional bootstrap prints only; the caller saves it after checking the broker result.
#[test]
#[ignore]
fn theme_generate_provisional() {
    for ((name, stop), rows) in cases() {
        for (token, value) in rows.iter().filter(|(key, _)| key.starts_with("input.")) {
            println!("THEME_GOLDEN\t{name}\t{stop}\t{token}\t{value}");
        }
        for (token, value) in resolve(&rows).tokens() {
            println!("THEME_GOLDEN\t{name}\t{stop}\t{token}\t{value}");
        }
    }
}
