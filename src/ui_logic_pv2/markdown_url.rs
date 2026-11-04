//! Linear URL decoding and image classification, with anchored scans and lexical folder containment.

use super::markdown_entity::decode_references;
use super::markdown_text::{at, scheme_len, trim};

const MAX_UNICODE_SCALAR: u32 = 0x10ffff;
// A target that still changes after this many decodings is refused: no real address nests that deep.
const MAX_DECODE_PASSES: usize = 8;

/// Where an image may load from: nowhere, the network (drawn as a placeholder) or a file inside the document's folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picture {
    Dropped,
    Remote(String),
    Local(String),
}

/// True for an http(s) URL or a protocol-relative one, which inherits https.
pub fn is_remote_url(url: &str) -> bool {
    let lower = url.as_bytes();
    let rest = if lower.len() >= 5 && lower[..5].eq_ignore_ascii_case(b"http:") {
        &url[5..]
    } else if lower.len() >= 6 && lower[..6].eq_ignore_ascii_case(b"https:") {
        &url[6..]
    } else {
        url
    };
    rest.starts_with("//")
}

/// The host a URL names, its user and port removed; a URL with no host answers itself.
pub fn host_of(url: &str) -> String {
    let mut rest = url;
    if let Some(len) = scheme_len(rest) {
        if rest[len..].starts_with("//") {
            rest = &rest[len + 2..];
        }
    }
    if let Some(stripped) = rest.strip_prefix("//") {
        rest = stripped;
    }
    let mut host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if let Some(at) = host.rfind('@') {
        host = &host[at + 1..];
    }
    if host.starts_with('[') {
        if let Some(close) = host.find(']').filter(|close| *close > 0) {
            return host[..=close].to_string();
        }
    }
    if let Some(colon) = host.find(':') {
        host = &host[..colon];
    }
    if host.is_empty() {
        url.to_string()
    } else {
        host.to_string()
    }
}

/// The folder part of a path, "" when it has no slash.
pub fn dir_of(path: &str) -> &str {
    path.rfind('/').map_or("", |slash| &path[..slash])
}

// "65583;" at 0 (after "&#") answers U+1002F and the index after ";".
fn numeric_ref(text: &str, i: usize) -> Option<(char, usize)> {
    let mut j = i;
    let hex = matches!(at(text, j), b'x' | b'X');
    if hex {
        j += 1;
    }
    let start = j;
    let bytes = text.as_bytes();
    while j < bytes.len()
        && (if hex {
            bytes[j].is_ascii_hexdigit()
        } else {
            bytes[j].is_ascii_digit()
        })
    {
        j += 1;
    }
    if j == start || at(text, j) != b';' {
        return None;
    }
    let digits = text[start..j].trim_start_matches('0');
    let code = if digits.len() > 8 {
        u32::MAX
    } else {
        u32::from_str_radix(
            if digits.is_empty() { "0" } else { digits },
            if hex { 16 } else { 10 },
        )
        .unwrap_or(u32::MAX)
    };
    if code == 0 || code > MAX_UNICODE_SCALAR {
        return None;
    }
    Some((char::from_u32(code).unwrap_or('\u{fffd}'), j + 1))
}

fn hex_value(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

/// The URL with numeric references and well-formed percent runs decoded and control characters dropped: "caf%C3%A9&#46;png" answers "café.png".
pub fn canonical_url(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'&' && at(raw, i + 1) == b'#' {
            if let Some((ch, end)) = numeric_ref(raw, i + 2) {
                out.push(ch);
                i = end;
                continue;
            }
            out.push('&');
            i += 1;
        } else if c == b'%'
            && i + 2 < bytes.len()
            && hex_value(bytes[i + 1]).is_some()
            && hex_value(bytes[i + 2]).is_some()
        {
            let begin = i;
            let mut decoded = Vec::new();
            loop {
                decoded.push(
                    hex_value(bytes[i + 1]).unwrap_or(0) * 16
                        + hex_value(bytes[i + 2]).unwrap_or(0),
                );
                i += 3;
                if !(at(raw, i) == b'%'
                    && i + 2 < bytes.len() + 1
                    && hex_value(at(raw, i + 1)).is_some()
                    && hex_value(at(raw, i + 2)).is_some())
                {
                    break;
                }
            }
            match String::from_utf8(decoded) {
                Ok(text) => out.push_str(&text),
                Err(_) => out.push_str(&raw[begin..i]),
            }
        } else {
            let ch = super::markdown_text::char_at(raw, i).unwrap_or('\u{fffd}');
            // Spaces survive: angle destinations may legally contain them. Tabs, newlines and other controls never do.
            if ch == ' ' || (ch as u32 > 32 && ch as u32 != 127) {
                out.push(ch);
            }
            i += ch.len_utf8();
        }
    }
    out
}

// "/../docs" clamps to "docs" for an absolute path; "../pic.png" is refused for a relative one.
fn normalize_subpath(name: &str, absolute: bool) -> Option<String> {
    let mut kept: Vec<&str> = Vec::new();
    for seg in name.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            if kept.pop().is_none() && !absolute {
                return None;
            }
            continue;
        }
        kept.push(seg);
    }
    if kept.is_empty() && !absolute {
        return None;
    }
    Some(kept.join("/"))
}

/// A path as a file URI, encoded as encodeURI does with "#" and "?" escaped too.
pub fn file_uri(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        let keep = b.is_ascii_alphanumeric()
            || matches!(
                b,
                b';' | b','
                    | b'/'
                    | b':'
                    | b'@'
                    | b'&'
                    | b'='
                    | b'+'
                    | b'$'
                    | b'-'
                    | b'_'
                    | b'.'
                    | b'!'
                    | b'~'
                    | b'*'
                    | b'\''
                    | b'('
                    | b')'
            );
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// An image target as a remote placeholder, a local file URL inside the document folder, or dropped alt text.
pub fn classify_image(raw: &str, dir: &str) -> Picture {
    if raw.is_empty() {
        return Picture::Dropped;
    }
    let seen = canonical_url(raw);
    if seen.is_empty() {
        return Picture::Dropped;
    }
    if is_remote_url(&seen) {
        return Picture::Remote(host_of(&seen));
    }
    // data:, file: and every other scheme never load unexamined.
    if scheme_len(&seen).is_some() {
        let starts = |text: &str, prefix: &[u8]| {
            text.len() >= prefix.len()
                && text.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix)
        };
        if starts(&seen, b"file:") {
            let mut path = seen.as_str();
            if starts(path, b"file://") {
                path = &path[7..];
            }
            if starts(path, b"file:") {
                path = &path[5..];
            }
            return local_absolute(path, dir);
        }
        return Picture::Dropped;
    }
    let mut name = seen.as_str();
    if name.len() >= 2 && name.starts_with('<') && name.ends_with('>') {
        name = &name[1..name.len() - 1];
    }
    if let Some(rest) = name.strip_prefix("./") {
        name = rest;
    }
    // An absolute path loads only inside the document's folder tree.
    if name.starts_with('/') || name.starts_with('\\') {
        return local_absolute(name, dir);
    }
    if name.is_empty() || name == "." || name == ".." || name.contains('\\') {
        return Picture::Dropped;
    }
    match normalize_subpath(name, false) {
        Some(collapsed) => contained_local(&format!("{dir}/{collapsed}"), dir),
        None => Picture::Dropped,
    }
}

// "/docs/notes/../pic.png" resolves inside /docs; "/docs/../pic.png" is refused.
fn local_absolute(path: &str, dir: &str) -> Picture {
    if path.contains('\\') {
        return Picture::Dropped;
    }
    contained_local(path, dir)
}

// The folder may hold a backslash (legal on Linux); only the reference the document wrote may not.
fn contained_local(path: &str, dir: &str) -> Picture {
    let root = if dir.is_empty() { "/" } else { dir };
    if !root.starts_with('/') || !path.starts_with('/') {
        return Picture::Dropped;
    }
    let (Some(base), Some(collapsed)) =
        (normalize_subpath(root, true), normalize_subpath(path, true))
    else {
        return Picture::Dropped;
    };
    if !base.is_empty() && collapsed != base && !collapsed.starts_with(&format!("{base}/")) {
        return Picture::Dropped;
    }
    Picture::Local(file_uri(&format!("/{collapsed}")))
}

/// The first srcset candidate that classifies local, else the first remote one, else None.
pub fn srcset_pick(value: &str, dir: &str) -> Option<Picture> {
    let mut remote = None;
    for part in value.split(',') {
        let cand = trim(part)
            .split(super::markdown_text::is_space)
            .next()
            .unwrap_or("");
        if cand.is_empty() {
            continue;
        }
        match classify_image(cand, dir) {
            Picture::Local(url) => return Some(Picture::Local(url)),
            Picture::Remote(host) if remote.is_none() => remote = Some(host),
            _ => {}
        }
    }
    remote.map(Picture::Remote)
}

/// The target without the control and space padding at its ends, and with every tab, CR and LF inside removed.
pub fn stripped_target(value: &str) -> String {
    value
        .trim_matches(|c: char| (c as u32) <= 0x20)
        .chars()
        .filter(|c| !matches!(c, '\t' | '\r' | '\n'))
        .collect()
}

/// True when the target, through every decoding, names http, https, mailto or no scheme at all.
pub fn target_allowed(url: &str) -> bool {
    let mut seen = url.to_string();
    for _ in 0..=MAX_DECODE_PASSES {
        let shown = stripped_target(&canonical_url(&seen));
        if let Some(len) = scheme_len(&shown) {
            let scheme = shown[..len].to_ascii_lowercase();
            if scheme != "http:" && scheme != "https:" && scheme != "mailto:" {
                return false;
            }
        }
        let next = decode_references(&shown);
        if next == seen || next == shown {
            return true;
        }
        seen = next;
    }
    false
}

/// True for a link that opens in the default application: an allowed target that names its scheme.
pub fn is_external_link(url: &str) -> bool {
    let seen = stripped_target(&canonical_url(url));
    scheme_len(&seen).is_some() && target_allowed(url)
}
