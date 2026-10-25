// The journal codec may omit known legacy fields, but must not discard unfamiliar structure.
use super::{checked_text, get, parse_u32, parse_u64};
use crate::jsondoc::Json;

pub(super) fn fields(pairs: &[(String, Json)], keys: &[&str]) -> Option<()> {
    pairs
        .iter()
        .all(|(key, _)| keys.contains(&key.as_str()))
        .then_some(())
}

pub(super) fn get_str<'a>(pairs: &'a [(String, Json)], key: &str) -> Option<&'a str> {
    get(pairs, key).and_then(Json::as_str)
}

pub(super) fn step(pairs: &[(String, Json)]) -> Option<()> {
    let kind = get_str(pairs, "k")?;
    let keys: &[&str] = match kind {
        "m" => &["k", "from", "to", "before", "after", "fat"],
        "c" => &["k", "path"],
        "l" => &["k", "path", "id", "src", "kind"],
        "cp" => &["k", "from", "to", "src", "made", "nonce", "merge", "fat"],
        "md" | "mf" => &["k", "path", "id"],
        "re" => &["k", "path", "mode"],
        "t" => &["k", "path", "uri", "before", "time"],
        "pm" | "tg" => &["k", "path", "before", "after", "dev", "ino", "born", "gen"],
        "barrier" => &["k"],
        _ => return None,
    };
    fields(pairs, keys)?;
    for name in ["fat", "merge"] {
        if let Some(value) = get(pairs, name) {
            if parse_u32(value) != Some(1) {
                return None;
            }
        }
    }
    if let Some(value) = get(pairs, "nonce") {
        parse_u64(value)?;
    }
    if kind == "t" {
        if let Some(value) = get(pairs, "time") {
            checked_text(value)?;
        }
        if let Some(value) = get(pairs, "before") {
            if !value.as_array()?.iter().all(|v| checked_text(v).is_some()) {
                return None;
            }
        }
    }
    Some(())
}
