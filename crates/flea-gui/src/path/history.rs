//! The jump's two window-side sources, parsed: the favourites ui.json keeps and the desktop's recent history, ui/js/Recent.js ported.
use flea::jsondoc::Json;
use std::path::{Path, PathBuf};

// The freedesktop recent file, and where XDG_DATA_HOME defaults to when the session did not set it.
const HISTORY_LEAF: &str = "recently-used.xbel";
// The newest this many paths are kept, Recent.js LIMIT.
pub const LIMIT: usize = 500;

/// The places favourites' paths, in the order the sidebar lists them.
pub fn favourites(state: &Json) -> Vec<String> {
    let records = state
        .get("places")
        .and_then(|places| places.get("favourites"))
        .and_then(Json::as_array)
        .unwrap_or(&[]);
    records
        .iter()
        .filter_map(|record| record.get("path").and_then(Json::as_str))
        .filter(|path| path.starts_with('/'))
        .map(str::to_owned)
        .collect()
}

/// Recent.historyPath: an XDG_DATA_HOME that is not absolute is no data home, so home's default answers for it.
pub fn history_path(data_home: Option<&str>, home: &str) -> PathBuf {
    match data_home.filter(|root| root.starts_with('/')) {
        Some(root) => Path::new(root).join(HISTORY_LEAF),
        None => Path::new(home).join(".local/share").join(HISTORY_LEAF),
    }
}

// Sample input: "file:///home/gm/a%20b.png" becomes "/home/gm/a b.png".
/// Recent.pathOf: a local file URI's path, refusing other schemes, foreign hosts, bad escapes and control characters.
pub fn path_of(href: &str) -> Option<String> {
    let scheme = href
        .get(..7)
        .filter(|scheme| scheme.eq_ignore_ascii_case("file://"))?;
    let rest = &href[scheme.len()..];
    let cut = rest.find('/')?;
    let authority = &rest[..cut];
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return None;
    }
    let path = super::decode(&rest[cut..])?;
    (path != "/" && !path.chars().any(|c| c.is_control() && (c as u32) < 0x80)).then_some(path)
}

// Sample input: "a&amp;b&#x41;" becomes "a&bA".
// The five predefined entities and character references; any other reference refuses the value.
fn unescape(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let end = rest[at..].find(';')? + at;
        let name = &rest[at + 1..end];
        let c = match name {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = match name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                    None => name.strip_prefix('#')?.parse().ok()?,
                };
                char::from_u32(code)?
            }
        };
        out.push(c);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

// Sample input: tag ' href="a&amp;b" visited="x"' and name "href" give "a&b".
// One start tag's attribute, unescaped; None when the tag does not carry it.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    loop {
        let at = rest.find(name)?;
        let before = rest[..at].chars().next_back();
        let after = rest[at + name.len()..].trim_start();
        rest = &rest[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let quote = value
            .chars()
            .next()
            .filter(|quote| *quote == '"' || *quote == '\'')?;
        let end = value[1..].find(quote)?;
        return unescape(&value[1..=end]);
    }
}

// Sample input: <bookmark href="file:///srv/a.txt" visited="2026-01-01T00:00:00Z"/>
/// Recent.entries over one xbel file: each bookmark's path, newest visit first, the file's order on a tie, each path once.
pub fn read(text: &str) -> Vec<String> {
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("<bookmark") {
        rest = &rest[at + "<bookmark".len()..];
        if !rest.starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/') {
            continue;
        }
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];
        rest = &rest[end..];
        let Some(path) = attribute(tag, "href").as_deref().and_then(path_of) else {
            continue;
        };
        // visited is when the file was last opened; the other two stand in for a writer that left it out.
        let stamp = ["visited", "modified", "added"]
            .iter()
            .find_map(|name| attribute(tag, name).filter(|stamp| !stamp.is_empty()));
        rows.push((stamp.unwrap_or_default(), path));
    }
    // A stable sort keeps the file's own order between two bookmarks that share a stamp.
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    let mut seen = std::collections::HashSet::new();
    rows.into_iter()
        .map(|(_, path)| path)
        .filter(|path| seen.insert(path.clone()))
        .take(LIMIT)
        .collect()
}
