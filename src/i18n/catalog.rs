use super::document::{self, MAX_BYTES};
use crate::jsondoc::Json;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

pub(super) type Catalog = HashMap<String, Json>;
pub(super) const KINDS: [&str; 4] = ["shell", "menu", "cli", "keybindings"];

// A replaced catalog must not make a UI reload block on a FIFO or read an unbounded file.
pub(super) fn read(path: &Path) -> Option<String> {
    let file = crate::backend::regfile::open_regular(path)?;
    if file.metadata().ok()?.len() > MAX_BYTES as u64 {
        eprintln!("flea: refusing i18n file {}: exceeds {MAX_BYTES} bytes", path.display());
        return None;
    }
    let mut text = String::new();
    file.take(MAX_BYTES as u64 + 1).read_to_string(&mut text).ok()?;
    if text.len() > MAX_BYTES {
        eprintln!("flea: refusing i18n file {}: exceeds {MAX_BYTES} bytes", path.display());
        return None;
    }
    Some(text)
}

// Sample input: {"folder.empty":"This folder is empty"} is a flat Flea catalog.
pub(super) fn parse(text: &str, menu: bool) -> Catalog {
    parse_checked(text, menu).unwrap_or_else(|error| {
        eprintln!("flea: refusing i18n catalog: {error}");
        Catalog::new()
    })
}

// Sample input: {"items":{"other":"%1 items"}}; menu mode consumes {"apps":{"label":"Apps"}}.
pub(super) fn parse_checked(text: &str, menu: bool) -> Result<Catalog, String> {
    let Json::Obj(pairs) = document::parse(text)? else {
        return Err("expected a catalog object".into());
    };
    let mut catalog = Catalog::new();
    for (key, value) in pairs {
        if menu {
            if let Json::Obj(fields) = value {
                for (field, text) in fields {
                    if matches!(text, Json::Str(_)) {
                        catalog.insert(format!("{key}.{field}"), text);
                    }
                }
            }
        } else {
            catalog.insert(key, value);
        }
    }
    Ok(catalog)
}

pub(super) fn load(path: &Path, menu: bool) -> Catalog {
    let Some(text) = read(path) else { return Catalog::new(); };
    parse_checked(&text, menu).unwrap_or_else(|error| {
        eprintln!("flea: refusing i18n catalog {}: {error}", path.display());
        Catalog::new()
    })
}

fn present(value: &Json) -> Option<&str> {
    value.as_str().filter(|s| !s.trim().is_empty())
}

fn pick<'a>(value: &'a Json, code: &str, count: Option<i64>) -> Option<&'a str> {
    if let Some(text) = present(value) {
        return Some(text);
    }
    let form = count.map(|n| super::plural::category(code, n)).unwrap_or("other");
    value.get(form).and_then(present).or_else(|| value.get("other").and_then(present))
}

// The English entry defines both key ownership and whether the value has plural forms.
pub(super) fn lookup<'a>(
    english: &'a Catalog, translated: &'a Catalog, key: &str, code: &str, count: Option<i64>,
) -> Option<&'a str> {
    let source = english.get(key)?;
    translated.get(key)
        .filter(|value| matches!((source, *value), (Json::Str(_), Json::Str(_)) | (Json::Obj(_), Json::Obj(_))))
        .and_then(|value| pick(value, code, count))
        .or_else(|| pick(source, "en", count))
}

// Scan once so %10 is distinct from %1 and replacement text is never interpreted as another hole.
// Sample input: "%2 then %1" with ["A", "B"] produces "B then A".
pub fn substitute(text: &str, args: &[&str]) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('%') {
        result.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = 1 + rest.as_bytes()[1..].iter().take_while(|b| b.is_ascii_digit()).count();
        let index = rest[1..end].parse::<usize>().ok().filter(|n| *n > 0);
        if let Some(value) = index.and_then(|n| args.get(n - 1)) {
            result.push_str(value);
        } else {
            result.push_str(&rest[..end]);
        }
        rest = &rest[end..];
    }
    result.push_str(rest);
    result
}
