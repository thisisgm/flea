//! The desktop's own recent history, read and never written; every bookmark is untrusted input.
use super::{decode_component, has_control};

pub const HISTORY_LEAF: &str = "recently-used.xbel";
pub const DEFAULT_DATA_HOME: &str = "/.local/share";
// The read stops here and hands the rail the newest LIMIT paths.
pub const LIMIT: usize = 500;

// Sample input: ("", "/home/gm") answers /home/gm/.local/share/recently-used.xbel; a relative data home is no data home.
pub fn history_path(data_home: &str, home: &str) -> String {
    let root = if data_home.starts_with('/') {
        data_home.to_owned()
    } else {
        format!("{home}{DEFAULT_DATA_HOME}")
    };
    format!("{}/{HISTORY_LEAF}", root.trim_end_matches('/'))
}

// Sample input: "file:///home/gm/a%20b.png" is "/home/gm/a b.png"; foreign schemes and hosts, bad escapes and control characters are None.
pub fn path_of(href: &str) -> Option<String> {
    let scheme = href.get(..7)?;
    if !scheme.eq_ignore_ascii_case("file://") {
        return None;
    }
    let rest = &href[7..];
    let cut = rest.find('/')?;
    let authority = &rest[..cut];
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return None;
    }
    let decoded = decode_component(&rest[cut..])?;
    if !decoded.starts_with('/') || decoded == "/" || has_control(&decoded) {
        return None;
    }
    Some(decoded)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Visit {
    pub path: String,
    pub stamp: String,
}

// Newest first with the stamps kept, the file's own order breaking ties, one row per path, at most LIMIT.
pub fn entries<'a>(bookmarks: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<Visit> {
    let mut rows: Vec<Visit> = bookmarks
        .into_iter()
        .filter_map(|(href, stamp)| {
            path_of(href).map(|path| Visit {
                path,
                stamp: stamp.to_owned(),
            })
        })
        .collect();
    rows.sort_by(|a, b| b.stamp.cmp(&a.stamp));
    let mut out: Vec<Visit> = Vec::new();
    for row in rows {
        if out.len() >= LIMIT {
            break;
        }
        if !out.iter().any(|kept| kept.path == row.path) {
            out.push(row);
        }
    }
    out
}

// Sample input: "/home/gm/a.txt" names "a.txt".
pub fn name_of(path: &str) -> &str {
    super::leaf(path)
}

// Sample input: "home/gm/a.txt" sits in "/home/gm", and "a.txt" in "/".
pub fn location_of(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    match path.rfind('/') {
        None | Some(0) => "/".to_owned(),
        Some(cut) if path.starts_with('/') => path[..cut].to_owned(),
        Some(cut) => format!("/{}", &path[..cut]),
    }
}

// Sample input: "/home/gm/Documents/claude/a.md" under "/home/gm" is "Documents/claude", a file in home "~", outside it absolute.
pub fn location_under(path: &str, home: &str) -> String {
    let location = location_of(path);
    if home.is_empty() {
        return location;
    }
    if location == home {
        return "~".to_owned();
    }
    match location
        .strip_prefix(home)
        .and_then(|rest| rest.strip_prefix('/'))
    {
        Some(rest) => rest.to_owned(),
        None => location,
    }
}

// Seconds since the epoch of an RFC 3339 UTC stamp such as 2026-08-30T11:32:04Z, None when it does not parse.
pub fn stamp_seconds(stamp: &str) -> Option<i64> {
    let b = stamp.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let num = |from: usize, to: usize| {
        stamp
            .get(from..to)
            .filter(|s| s.bytes().all(|c| c.is_ascii_digit()))?
            .parse::<i64>()
            .ok()
    };
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    let mut rest = &stamp[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let digits = frac.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        rest = &frac[digits..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        zone if zone.len() == 6
            && (zone.starts_with('+') || zone.starts_with('-'))
            && &zone[3..4] == ":" =>
        {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            sign * (zone[1..3].parse::<i64>().ok()? * 3600 + zone[4..6].parse::<i64>().ok()? * 60)
        }
        _ => return None,
    };
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    // Days from the civil date, Howard Hinnant's algorithm.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

// The Used column's visit per path, one bounded lookup beside the entries, None where a stamp does not parse.
pub fn visits(entries: &[Visit]) -> super::recentmode::Visits {
    entries
        .iter()
        .map(|visit| (visit.path.clone(), stamp_seconds(&visit.stamp)))
        .collect()
}

// Each asker waits once on the read in flight; None, the rail pane itself, joins like any other.
pub fn join_requesters<T: PartialEq + Clone>(
    current: &[Option<T>],
    requester: Option<T>,
) -> Vec<Option<T>> {
    let mut out = current.to_vec();
    if !out.contains(&requester) {
        out.push(requester);
    }
    out
}

fn xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

// Character and the five named entity references decode; an unknown one stays as written.
fn decode_entities(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut tail = body;
    while let Some(amp) = tail.find('&') {
        out.push_str(&tail[..amp]);
        let after = &tail[amp..];
        let Some(semi) = after.find(';') else { break };
        let entity = &after[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .map(|hex| u32::from_str_radix(hex, 16))
                .or_else(|| entity.strip_prefix('#').map(str::parse))
                .and_then(Result::ok)
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&after[..=semi]),
        }
        tail = &after[semi + 1..];
    }
    out.push_str(tail);
    out
}

// Sample input: ` href = "file:///a%20b"\tvisited='2026-08-30T11:32:04Z'`; any XML space may separate names, = and values.
fn attributes(mut body: &str) -> Vec<(&str, String)> {
    let mut out = Vec::new();
    loop {
        body = body.trim_start_matches(xml_space);
        let name_end = body
            .find(|c: char| c == '=' || xml_space(c))
            .unwrap_or(body.len());
        if name_end == 0 {
            return out;
        }
        let name = &body[..name_end];
        let Some(rest) = body[name_end..]
            .trim_start_matches(xml_space)
            .strip_prefix('=')
        else {
            return out;
        };
        let rest = rest.trim_start_matches(xml_space);
        let Some(quote) = rest.chars().next().filter(|q| *q == '"' || *q == '\'') else {
            return out;
        };
        let Some(end) = rest[1..].find(quote) else {
            return out;
        };
        out.push((name, decode_entities(&rest[1..=end])));
        body = &rest[end + 2..];
    }
}

// The '>' that ends a tag, skipping any inside a quoted attribute value.
fn tag_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (at, c) in text.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if q == c => quote = None,
            (None, '>') => return Some(at),
            _ => {}
        }
    }
    None
}

// Each /xbel/bookmark's href and stamp (visited, else modified, else added), as v0.3.12's XML reader walks them; a broken close stops the read.
pub fn bookmarks(xbel: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut open: Vec<&str> = Vec::new();
    let mut rest = xbel;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt..];
        let skip = [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>")]
            .into_iter()
            .find(|(start, _)| rest.starts_with(start));
        if let Some((start, end)) = skip {
            rest = rest[start.len()..]
                .find(end)
                .map_or("", |at| &rest[start.len() + at + end.len()..]);
            continue;
        }
        let Some(end) = tag_end(rest) else { break };
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            if open.pop() != Some(name.trim_end_matches(xml_space)) {
                break;
            }
            continue;
        }
        let closed = tag.ends_with('/');
        let body = tag.strip_suffix('/').unwrap_or(tag);
        let name_end = body.find(xml_space).unwrap_or(body.len());
        let name = &body[..name_end];
        if name == "bookmark" && open == ["xbel"] {
            let attrs = attributes(&body[name_end..]);
            let value = |key: &str| {
                attrs
                    .iter()
                    .find(|(n, _)| *n == key)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            };
            let stamp = ["visited", "modified", "added"]
                .into_iter()
                .map(value)
                .find(|s| !s.is_empty())
                .unwrap_or_default();
            out.push((value("href"), stamp));
        }
        if !closed {
            open.push(name);
        }
    }
    out
}
