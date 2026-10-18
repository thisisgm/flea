use super::Color;
use std::collections::BTreeMap;

pub type Values = BTreeMap<String, String>;

fn key(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

// Palette.parse and Color.loadColors accept the leading six hex digits, including unquoted values.
pub fn parse_colors(raw: &str) -> BTreeMap<String, Color> {
    let mut out = BTreeMap::new();
    for line in raw.lines() {
        let Some((name, value)) = line.trim_start().split_once('=') else {
            continue;
        };
        let name = name.trim_end();
        let value = value.trim_start();
        let value = value.strip_prefix(['\'', '"']).unwrap_or(value);
        if key(name) {
            if let Some(c) = value.get(..7).and_then(Color::parse) {
                out.insert(name.to_owned(), c);
            }
        }
    }
    out
}

// A small walker for the exact subset Color.parseShell accepts, not a general TOML parser.
pub fn parse_shell(raw: &str) -> Values {
    shell_entries(raw).into_iter().collect()
}

pub(super) fn shell_entries(raw: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut section = "";
    for line in raw.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            if let Some((name, tail)) = rest.split_once(']') {
                if key(name) && comment(tail) {
                    section = name;
                    continue;
                }
            }
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim_end();
        if !key(name) || section.is_empty() {
            continue;
        }
        let value = value.trim_start();
        let parsed = if value.starts_with(['\'', '"']) {
            value[1..].find(['\'', '"']).and_then(|i| {
                let (body, tail) = value[1..].split_at(i);
                (!body.is_empty() && comment(&tail[1..])).then_some(body)
            })
        } else {
            let body = value.split('#').next().unwrap_or("").trim_end();
            let parts: Vec<_> = body.split_whitespace().collect();
            let numeric = !parts.is_empty() && parts.len() <= 4 && parts.iter().all(|s| number(s));
            let role = body.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) && key(body);
            let bare_tail_ok = !value.contains('#') || comment(&value[body.len()..]);
            ((numeric || role) && bare_tail_ok).then_some(body)
        };
        if let Some(v) = parsed {
            let full = format!("{section}.{name}");
            if let Some((_, old)) = out.iter_mut().find(|(key, _)| *key == full) {
                *old = v.to_owned();
            } else {
                out.push((full, v.to_owned()));
            }
        }
    }
    out
}

pub(super) fn merge_shell(theme: &str, user: &str) -> Vec<(String, String)> {
    let mut out = shell_entries(theme);
    for (key, value) in shell_entries(user) {
        if let Some((_, old)) = out.iter_mut().find(|(k, _)| *k == key) {
            *old = value;
        } else {
            out.push((key, value));
        }
    }
    out
}

fn number(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let mut parts = s.split('.');
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    digits(parts.next().unwrap_or(""))
        && parts.next().map(digits).unwrap_or(true)
        && parts.next().is_none()
}

fn comment(s: &str) -> bool {
    let s = s.trim_start();
    s.is_empty() || s.starts_with('#')
}

pub(super) fn finite(values: &Values, key: &str, fallback: f64) -> f64 {
    let Some(raw) = values.get(key) else {
        return fallback;
    };
    // Qt's Number conversion trims Unicode whitespace, including NEL but excluding BOM.
    let s = raw.trim();
    if s.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ] {
        if let Some(digits) = s.strip_prefix(prefix) {
            // Qt converts the radix suffix through a signed integer with ASCII whitespace.
            let digits = digits.trim_matches(|c| matches!(c, '\t'..='\r' | ' '));
            return i64::from_str_radix(digits, radix)
                .map(|n| n as f64)
                .unwrap_or(fallback);
        }
    }
    s.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .unwrap_or(fallback)
}

fn js_space(c: char) -> bool {
    matches!(c,
        '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}' | '\u{feff}')
}

// Commons parses spacing with JavaScript parseFloat, consuming the longest decimal prefix.
pub(super) fn float_prefix(values: &Values, key: &str, fallback: f64) -> f64 {
    let Some(raw) = values.get(key) else {
        return fallback;
    };
    let s = raw.trim_start_matches(js_space);
    let bytes = s.as_bytes();
    let mut at = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let mut digits = 0;
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
        digits += 1;
    }
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return fallback;
    }
    let mut end = at;
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let exponent = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at > exponent {
            end = at;
        }
    }
    s[..end]
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .unwrap_or(fallback)
}

pub(super) fn integer(values: &Values, key: &str) -> Option<f64> {
    // Style.applyShellValues uses parseInt, so a quoted "14px" is still 14.
    let s = values.get(key)?.trim_start();
    let n = s.strip_prefix(['-', '+']).unwrap_or(s);
    let digits = n.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let end = s.len() - n.len() + digits;
    s[..end].parse::<f64>().ok().filter(|n| n.is_finite())
}

pub(super) fn boolean(values: &Values, key: &str, fallback: bool) -> bool {
    match values
        .get(key)
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("true" | "1" | "yes" | "on") => true,
        Some("false" | "0" | "no" | "off") => false,
        _ => fallback,
    }
}
