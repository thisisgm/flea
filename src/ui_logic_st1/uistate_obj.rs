// JS object semantics over crate::jsondoc::Json: insertion order, for-in keys, and JSON.stringify equality.
use crate::jsondoc::Json;

// The keys a JS for-in visits in order (pairs, indices, code units); a non-BMP character's units read as U+FFFD.
pub(super) fn keys_of(value: &Json) -> Vec<(String, Json)> {
    match value {
        Json::Obj(pairs) => pairs.clone(),
        Json::Arr(items) => items.iter().enumerate().map(|(i, v)| (i.to_string(), v.clone())).collect(),
        Json::Str(text) => text
            .encode_utf16()
            .enumerate()
            .map(|(i, unit)| (i.to_string(), Json::Str(String::from_utf16_lossy(&[unit]))))
            .collect(),
        _ => Vec::new(),
    }
}

// JS assignment: an existing key keeps its position, a new key is appended.
pub(super) fn put(pairs: &mut Vec<(String, Json)>, key: &str, value: Json) {
    if let Some(i) = pairs.iter().position(|(k, _)| k == key) {
        pairs[i].1 = value;
    } else {
        pairs.push((key.to_string(), value));
    }
}

// JS delete: removes the key when present.
pub(super) fn del(pairs: &mut Vec<(String, Json)>, key: &str) {
    pairs.retain(|(k, _)| k != key);
}

// Reads a key of a pair list, the way `obj[key]` reads an own property.
pub(super) fn find<'a>(pairs: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

// JSON.stringify(a) === JSON.stringify(b), where None stands for undefined.
pub(super) fn same(a: Option<&Json>, b: Option<&Json>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => same_json(x, y),
        _ => false,
    }
}

fn same_json(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Null, Json::Null) => true,
        (Json::Bool(x), Json::Bool(y)) => x == y,
        // JS compares numbers by value, so 1.0 and 1 are the same written document.
        (Json::Num(_), Json::Num(_)) => a.as_f64() == b.as_f64(),
        (Json::Str(x), Json::Str(y)) => x == y,
        (Json::Arr(x), Json::Arr(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_json(p, q)),
        (Json::Obj(x), Json::Obj(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|((k1, v1), (k2, v2))| k1 == k2 && same_json(v1, v2))
        }
        _ => false,
    }
}

// JS truthiness: null, false, zero and the empty string are falsy, and so is nothing else.
pub(super) fn truthy(value: &Json) -> bool {
    match value {
        Json::Null => false,
        Json::Bool(b) => *b,
        Json::Num(_) => value.as_f64().is_some_and(|n| n != 0.0),
        Json::Str(s) => !s.is_empty(),
        Json::Arr(_) | Json::Obj(_) => true,
    }
}
