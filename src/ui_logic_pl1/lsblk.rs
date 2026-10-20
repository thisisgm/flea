//! Field reads over one lsblk --json node, with the coercions the v0.3.12 rail applied.
use crate::jsondoc::Json;

/// A field as text the way the rail read it: absent, null, false, zero and "" read as "".
pub fn text(node: &Json, key: &str) -> String {
    node.get(key).map(string_of).unwrap_or_default()
}

/// A truthy field rendered as text, else "".
pub fn string_of(value: &Json) -> String {
    if !truthy(value) {
        return String::new();
    }
    match value {
        Json::Str(s) => s.clone(),
        Json::Num(n) => n.clone(),
        Json::Bool(_) => "true".into(),
        _ => String::new(),
    }
}

/// Script truthiness: null, false, zero and "" are false; arrays and objects are true.
pub fn truthy(value: &Json) -> bool {
    match value {
        Json::Null => false,
        Json::Bool(b) => *b,
        Json::Num(n) => n.parse::<f64>().map(|v| v != 0.0 && !v.is_nan()).unwrap_or(true),
        Json::Str(s) => !s.is_empty(),
        Json::Arr(_) | Json::Obj(_) => true,
    }
}

/// Whether a field is present and truthy.
pub fn has(node: &Json, key: &str) -> bool {
    node.get(key).is_some_and(truthy)
}

/// Whether a field is the boolean true itself, never a "1" or 1.
pub fn is_true(node: &Json, key: &str) -> bool {
    matches!(node.get(key), Some(Json::Bool(true)))
}

/// The node's children, empty when absent or not an array.
pub fn children(node: &Json) -> &[Json] {
    node.get("children").and_then(Json::as_array).unwrap_or(&[])
}

/// Each mountpoint, a null one as "".
pub fn mountpoints(node: &Json) -> Vec<String> {
    let Some(points) = node.get("mountpoints").and_then(Json::as_array) else { return Vec::new() };
    points
        .iter()
        .map(|p| match p {
            Json::Null => String::new(),
            Json::Str(s) => s.clone(),
            Json::Num(n) => n.clone(),
            Json::Bool(b) => b.to_string(),
            _ => String::new(),
        })
        .collect()
}

/// The first mountpoint that is a real path; swap and an unmounted volume answer "".
pub fn mount_of(node: &Json) -> String {
    mountpoints(node).into_iter().find(|p| p.starts_with('/')).unwrap_or_default()
}

/// lsblk's own PATH, else /dev/ and the kernel name.
pub fn device_path(node: &Json) -> String {
    if has(node, "path") {
        return text(node, "path");
    }
    format!("/dev/{}", raw_name(node))
}

/// The NAME column rendered even when null, as the rail's String(name) did.
pub fn raw_name(node: &Json) -> String {
    match node.get("name") {
        None => "undefined".into(),
        Some(Json::Null) => "null".into(),
        Some(Json::Bool(b)) => b.to_string(),
        Some(Json::Str(s)) => s.clone(),
        Some(Json::Num(n)) => n.clone(),
        Some(_) => String::new(),
    }
}

/// A capacity that is a safe non-negative integer, else absent.
pub fn bytes(node: &Json) -> Option<u64> {
    let Some(Json::Num(_)) = node.get("size") else { return None };
    let value = node.get("size")?.as_f64()?;
    let safe = value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_991.0;
    (safe && value >= 0.0).then_some(value as u64)
}

/// The top-level nodes of a listing, or None when the body is not JSON at all.
pub fn parse_nodes(body: &str) -> Option<Vec<Json>> {
    let tree = crate::jsondoc::parse(body).ok()?;
    Some(tree.get("blockdevices").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default())
}

/// parseInt(text, 16): leading space, an optional sign and 0x, then hex digits; None when none lead.
pub fn parse_hex(text: &str) -> Option<i64> {
    let trimmed = text.trim_start();
    let (negative, rest) = match trimmed.as_bytes().first() {
        Some(b'-') => (true, &trimmed[1..]),
        Some(b'+') => (false, &trimmed[1..]),
        _ => (false, trimmed),
    };
    let rest = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")).unwrap_or(rest);
    let digits: String = rest.chars().take_while(char::is_ascii_hexdigit).collect();
    if digits.is_empty() {
        return None;
    }
    let value = i64::from_str_radix(&digits[..digits.len().min(15)], 16).ok()?;
    Some(if negative { -value } else { value })
}
