//! The clipboard payload bytes, one place for every front end: GNOME's copy/cut form, the URI list, plain paths, KDE's cut mark and Flea's token; byte for byte the backend shapes (src/clip/format.rs) and the fork MIME contract. X11 stays out.
use crate::plugin_runtime::protocol::place_path;
use std::collections::HashSet;

pub const GNOME: &str = "x-special/gnome-copied-files";
pub const URILIST: &str = "text/uri-list";
pub const KDE_CUT: &str = "application/x-kde-cutselection";
pub const PLAIN_UTF8: &str = "text/plain;charset=utf-8";
pub const PLAIN: &str = "text/plain";
pub const UTF8_STRING: &str = "UTF8_STRING";
pub const FLEA: &str = "application/x-flea-clip";

// Keep a place's scheme and percent-encode its path just as for a local file.
fn encode_uri(path: &str) -> String {
    let (mut out, path) = match place_path(path) {
        Some((id, relative)) => (format!("plugin://{id}"), relative),
        None => (String::from("file://"), path),
    };
    for byte in path.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// Sample input: "file:///tmp/a%20b" reads "/tmp/a b"; "file://host/x" and "file:///tmp/%+1" read None.
// Local files and validated plugin locations retain their distinct owners.
fn decode_uri(uri: &[u8]) -> Option<String> {
    let plugin = uri.starts_with(b"plugin://");
    let scheme: &[u8] = if plugin { b"plugin://" } else { b"file://" };
    let rest = uri.strip_prefix(scheme)?;
    let slash = rest.iter().position(|byte| *byte == b'/')?;
    let (host, path) = rest.split_at(slash);
    if !plugin && !host.is_empty() && host != b"localhost" {
        return None;
    }
    let mut raw = Vec::with_capacity(path.len());
    let mut at = 0;
    while at < path.len() {
        if path[at] == b'%' {
            let hex = path.get(at + 1..at + 3)?;
            // from_str_radix takes a leading '+', so only two hex digits are an escape.
            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            raw.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            at += 3;
        } else {
            raw.push(path[at]);
            at += 1;
        }
    }
    if raw.contains(&0) {
        return None;
    }
    let path = String::from_utf8(raw).ok()?;
    if !path.starts_with('/') || path.split('/').any(|part| part == "." || part == "..") {
        return None;
    }
    if plugin {
        let uri = format!("plugin://{}{path}", std::str::from_utf8(host).ok()?);
        place_path(&uri)?;
        Some(uri)
    } else {
        Some(path)
    }
}

pub fn is_op(word: &str) -> bool {
    word == "copy" || word == "cut"
}

// The token's length in hex chars, the backend's TOKEN_HEX_LEN (src/clip/format.rs), which this library crate cannot import.
const TOKEN_HEX_LEN: usize = 32;

// The token rule both validators share: TOKEN_HEX_LEN hex chars, the form make_token writes.
pub fn is_token(token: &str) -> bool {
    token.len() == TOKEN_HEX_LEN && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn build_gnome(op: &str, paths: &[String]) -> Vec<u8> {
    let mut out = op.to_string();
    for path in paths {
        out.push('\n');
        out.push_str(&encode_uri(path));
    }
    out.into_bytes()
}

// Sample input "copy\nfile:///tmp/a\n" gives ("copy", ["/tmp/a"], 0); a first line other than copy or cut is None.
pub fn parse_gnome(bytes: &[u8]) -> Option<(String, Vec<String>, usize)> {
    let mut lines = bytes.split(|byte| *byte == b'\n');
    let op = match lines.next()? {
        b"copy" => "copy",
        b"cut" => "cut",
        _ => return None,
    };
    let (paths, skipped) = collect(lines.filter(|line| !line.is_empty()));
    Some((op.to_string(), paths, skipped))
}

pub fn build_urilist(paths: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for path in paths {
        out.extend_from_slice(encode_uri(path).as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out
}

// Sample input: "file:///tmp/a%20b\r\n# note\r\n" reads (["/tmp/a b"], 0).
// CRLF and LF both read; '#' lines are comments and never files.
pub fn parse_urilist(bytes: &[u8]) -> (Vec<String>, usize) {
    collect(bytes.split(|byte| *byte == b'\n').filter_map(|line| {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() || line.starts_with(b"#") {
            return None;
        }
        Some(line)
    }))
}

// The absolute paths, one a line, the form ui/js/Drag.js offers beside its uri-list.
pub fn build_plain(paths: &[String]) -> Vec<u8> {
    paths.join("\n").into_bytes()
}

pub fn build_kde_cut() -> Vec<u8> {
    b"1".to_vec()
}

// Sample input: "1\0" reads true; "0" or anything else reads false.
pub fn parse_kde_cut(bytes: &[u8]) -> bool {
    String::from_utf8_lossy(bytes).trim_matches(|c: char| c == '\0' || c.is_whitespace()) == "1"
}

pub fn build_flea(op: &str, token: &str) -> Vec<u8> {
    format!("{op} {token} {}", std::process::id()).into_bytes()
}

// Sample input: "cut 0123456789abcdef0123456789abcdef 4242" reads ("cut", that token).
// The token is TOKEN_HEX_LEN hex chars; anything else is another owner's bytes, not a refusal.
pub fn parse_flea(bytes: &[u8]) -> Option<(String, String)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    let (op, token) = text.split_once(' ')?;
    let token = token.split_once(' ').map_or(token, |(token, _)| token);
    if !is_op(op) || !is_token(token) {
        return None;
    }
    Some((op.to_string(), token.to_string()))
}

// Shared by both list shapes: refused URIs count as skipped, never fail the read.
fn collect<'a>(lines: impl Iterator<Item = &'a [u8]>) -> (Vec<String>, usize) {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    let mut skipped = 0;
    for line in lines {
        match decode_uri(line) {
            Some(path) if seen.insert(path.clone()) => paths.push(path),
            _ => skipped += 1,
        }
    }
    (paths, skipped)
}

/// One offered MIME representation read off a foreign selection.
pub struct Offer<'a> {
    pub mime: &'a str,
    pub bytes: &'a [u8],
}

/// A foreign file selection: GNOME first for its cut intent, then a URI list with KDE's cut mark, then nothing; plain text alone is never files, so the text clipboard and the URI identity stay distinct.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForeignSel {
    pub clip: String,
    pub paths: Vec<String>,
    pub token: String,
}

// Sample input: GNOME cut bytes with our token read ("cut", paths, token).
pub fn choose(offers: &[Offer<'_>]) -> Option<ForeignSel> {
    let bytes_of = |mime: &str| {
        offers
            .iter()
            .find(|offer| offer.mime == mime)
            .map(|offer| offer.bytes)
    };
    let token = bytes_of(FLEA)
        .and_then(parse_flea)
        .map_or(String::new(), |(_, token)| token);
    // A GNOME offer that does not parse or names no files falls through to the URI list, as the backend reads it.
    if let Some((op, paths, _)) = bytes_of(GNOME)
        .and_then(parse_gnome)
        .filter(|(_, paths, _)| !paths.is_empty())
    {
        return Some(ForeignSel {
            clip: op,
            paths,
            token,
        });
    }
    if let Some(bytes) = bytes_of(URILIST) {
        let (paths, _) = parse_urilist(bytes);
        if paths.is_empty() {
            return None;
        }
        let cut = bytes_of(KDE_CUT).is_some_and(parse_kde_cut);
        let clip = if cut { "cut" } else { "copy" };
        return Some(ForeignSel {
            clip: clip.into(),
            paths,
            token,
        });
    }
    None
}
