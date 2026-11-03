use super::{stamp, uri, BYTES, RECORDS};
use std::collections::BTreeMap;

fn space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn characters(text: &str) -> bool {
    text.chars().all(|c| space(c) || c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}')
}

fn name(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, b':' | b'_'))
        && bytes.all(|c| c.is_ascii_alphanumeric() || matches!(c, b':' | b'_' | b'-' | b'.'))
}

// No DTD or entity declarations are accepted; only the five XML entities and numeric character references are local.
fn value(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        let end = rest.find(';')?;
        let entity = &rest[..end];
        out.push(match entity {
            "amp" => '&', "lt" => '<', "gt" => '>', "quot" => '"', "apos" => '\'',
            _ => {
                let (digits, radix) = if let Some(hex) = entity.strip_prefix("#x") { (hex, 16) }
                    else { (entity.strip_prefix('#')?, 10) };
                if digits.is_empty() || !digits.bytes().all(|b| if radix == 16 { b.is_ascii_hexdigit() } else { b.is_ascii_digit() }) {
                    return None;
                }
                char::from_u32(u32::from_str_radix(digits, radix).ok()?)?
            }
        });
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    characters(&out).then_some(out)
}

fn attribute(text: &str) -> Option<(&str, &str, &str)> {
    if !text.starts_with(space) { return None; }
    let text = text.trim_start_matches(space);
    let at = text.find(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, ':' | '_' | '-' | '.')).unwrap_or(text.len());
    let key = &text[..at];
    if !name(key) { return None; }
    let text = text[at..].trim_start_matches(space).strip_prefix('=')?.trim_start_matches(space);
    let quote = text.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    let text = &text[1..];
    let end = text.find(quote)?;
    if text[..end].contains('<') { return None; }
    Some((key, &text[..end], &text[end + 1..]))
}

fn attributes(mut text: &str) -> Option<BTreeMap<&str, String>> {
    let mut out = BTreeMap::new();
    while !text.trim_start_matches(space).is_empty() {
        let (key, raw, rest) = attribute(text)?;
        if out.len() >= 32 || out.insert(key, value(raw)?).is_some() { return None; }
        text = rest;
    }
    Some(out)
}

fn declaration(text: &str) -> Option<()> {
    let (key, version, mut rest) = attribute(text)?;
    if (key, version) != ("version", "1.0") { return None; }
    let (mut encoding, mut standalone) = (false, false);
    while !rest.trim_start_matches(space).is_empty() {
        let (key, raw, tail) = attribute(rest)?;
        match key {
            "encoding" if !encoding && !standalone && raw.eq_ignore_ascii_case("UTF-8") => encoding = true,
            "standalone" if !standalone && matches!(raw, "yes" | "no") => standalone = true,
            _ => return None,
        }
        rest = tail;
    }
    Some(())
}

pub(super) fn xbel(text: &str) -> Option<Vec<Vec<u8>>> {
    // Validate the entire bounded file, including ignored constructs and content after the record cap.
    if text.len() > BYTES || !characters(text) { return None; }
    let document = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rest = document;
    let mut stack = Vec::new();
    let mut root = false;
    let mut rows = Vec::new();
    while !rest.is_empty() {
        if !rest.starts_with('<') {
            let end = rest.find('<').unwrap_or(rest.len());
            let raw = &rest[..end];
            if raw.contains("]]>") || stack.is_empty() && !raw.chars().all(space) { return None; }
            value(raw)?;
            rest = &rest[end..];
            continue;
        }
        if rest.starts_with("<!--") {
            let end = rest[4..].find("-->")? + 4;
            let body = &rest[4..end];
            if body.contains("--") || body.ends_with('-') { return None; }
            rest = &rest[end + 3..];
            continue;
        }
        if rest.starts_with("<?") {
            let end = rest[2..].find("?>")? + 2;
            let body = &rest[2..end];
            let at = body.find(space).unwrap_or(body.len());
            let target = &body[..at];
            if !name(target) { return None; }
            if target.eq_ignore_ascii_case("xml") {
                if target != "xml" || rest.len() != document.len() { return None; }
                declaration(&body[at..])?;
            }
            rest = &rest[end + 2..];
            continue;
        }
        if rest.starts_with("<![CDATA[") && !stack.is_empty() {
            let end = rest[9..].find("]]>")? + 9;
            rest = &rest[end + 3..];
            continue;
        }
        if rest.starts_with("<!") { return None; }
        let mut quote = None;
        let end = rest[1..].char_indices().find_map(|(at, c)| {
            if quote == Some(c) { quote = None; }
            else if quote.is_none() && matches!(c, '\'' | '"') { quote = Some(c); }
            else if quote.is_none() && c == '>' { return Some(at + 1); }
            None
        })?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if let Some(key) = tag.strip_prefix('/') {
            if !name(key.trim_end_matches(space)) || stack.pop() != Some(key.trim_end_matches(space)) { return None; }
            continue;
        }
        let empty = tag.ends_with('/');
        let tag = if empty { &tag[..tag.len() - 1] } else { tag };
        let at = tag.find(space).unwrap_or(tag.len());
        let key = &tag[..at];
        if !name(key) { return None; }
        let attrs = attributes(&tag[at..])?;
        if stack.is_empty() {
            if root || key != "xbel" { return None; }
            root = true;
        }
        if key == "bookmark" && stack.as_slice() == ["xbel"] && rows.len() < RECORDS {
            if let Some(path) = attrs.get("href").and_then(|href| uri(href)) {
                let recent = ["visited", "modified", "added"].into_iter().find_map(|key| attrs.get(key));
                let recent = match recent { Some(text) => stamp(text), None => Some((i64::MIN, 0)) };
                if let Some(recent) = recent { rows.push((path, recent)); }
            }
        }
        if !empty { if stack.len() >= 32 { return None; } stack.push(key); }
    }
    if !root || !stack.is_empty() { return None; }
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Some(rows.into_iter().map(|(path, _)| path).collect())
}
