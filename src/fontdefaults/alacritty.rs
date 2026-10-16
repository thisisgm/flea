// Surgical TOML edits: lexical boundaries protect strings, comments and unrelated tables.
// Only font family fields are changed; unsupported/ambiguous font layouts are refused.
use std::collections::{BTreeMap, HashSet};
#[derive(Clone)]
struct Token { text: String, start: usize, end: usize }
fn error() -> String { "This Alacritty layout needs manual editing; no preferences were changed.".into() }
fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'#' { while i < b.len() && b[i] != b'\n' { i += 1; } continue; }
        if matches!(b[i], b' ' | b'\t' | b'\r') { i += 1; continue; }
        let start = i;
        if matches!(b[i], b'\'' | b'"') {
            let quote = b[i];
            let triple = b.get(i..i + 3).map(|s| s == [quote; 3]).unwrap_or(false);
            let width = if triple { 3 } else { 1 };
            i += width;
            loop {
                if i >= b.len() { return Err(error()); }
                if quote == b'"' && b[i] == b'\\' { i += 2; continue; }
                if b.get(i..i + width).map(|s| s.iter().all(|v| *v == quote)).unwrap_or(false) { i += width; break; }
                if !triple && b[i] == b'\n' { return Err(error()); }
                i += 1;
            }
        } else if b"[]=,{}.\n".contains(&b[i]) { i += 1; }
        else { while i < b.len() && !b" \t\r\n#[]=,{}.'\"".contains(&b[i]) { i += 1; } }
        if i == start || i > b.len() { return Err(error()); }
        out.push(Token { text: text[start..i].into(), start, end: i });
    }
    Ok(out)
}
fn name(token: &Token) -> Result<String, String> {
    let s = &token.text;
    if s.starts_with('"') {
        return crate::jsondoc::parse(s)?.as_str().map(str::to_string).ok_or_else(error);
    }
    if let Some(s) = s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) { return Ok(s.into()); }
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)) { return Err(error()); }
    Ok(s.clone())
}
fn keys(ts: &[Token], i: &mut usize, end: &str) -> Result<Vec<String>, String> {
    let mut out = vec![name(ts.get(*i).ok_or_else(error)?)?];
    *i += 1;
    while ts.get(*i).map(|t| t.text.as_str()) == Some(".") {
        *i += 1;
        out.push(name(ts.get(*i).ok_or_else(error)?)?);
        *i += 1;
    }
    if ts.get(*i).map(|t| t.text.as_str()) != Some(end) { return Err(error()); }
    *i += 1;
    Ok(out)
}
struct Container { path: Vec<String>, at: usize, inline: bool, comma: bool }
struct Document { values: Vec<(Vec<String>, usize, usize)>, containers: Vec<Container> }
fn value(ts: &[Token], i: &mut usize, path: Vec<String>, doc: &mut Document, depth: usize) -> Result<(), String> {
    if depth > 32 { return Err(error()); }
    let start = ts.get(*i).ok_or_else(error)?.start;
    if ts[*i].text == "{" {
        *i += 1;
        let mut count = 0;
        let mut comma = false;
        while ts.get(*i).map(|t| t.text.as_str()) != Some("}") {
            let mut child = path.clone();
            child.extend(keys(ts, i, "=")?);
            value(ts, i, child, doc, depth + 1)?;
            count += 1;
            if ts.get(*i).map(|t| t.text.as_str()) == Some(",") { *i += 1; comma = true; }
            else { comma = false; if ts.get(*i).map(|t| t.text.as_str()) != Some("}") { return Err(error()); } }
        }
        doc.containers.push(Container { path: path.clone(), at: ts[*i].start, inline: true, comma: count > 0 && !comma });
        *i += 1;
    } else {
        let mut brackets = 0usize;
        while let Some(token) = ts.get(*i) {
            let s = token.text.as_str();
            if brackets == 0 && ["\n", ",", "}"].contains(&s) { break; }
            if s == "[" { brackets += 1; }
            if s == "]" { brackets = brackets.checked_sub(1).ok_or_else(error)?; }
            *i += 1;
        }
        if brackets != 0 { return Err(error()); }
    }
    let end = ts.get(i.saturating_sub(1)).ok_or_else(error)?.end;
    if end <= start { return Err(error()); }
    doc.values.push((path, start, end));
    Ok(())
}
fn parse(text: &str) -> Result<Document, String> {
    let ts = tokens(text)?;
    let mut doc = Document { values: Vec::new(), containers: Vec::new() };
    let mut section = Vec::new();
    let mut section_at = None;
    let mut tables = HashSet::new();
    let mut i = 0;
    while i < ts.len() {
        if ts[i].text == "\n" { i += 1; continue; }
        if ts[i].text == "[" {
            if let Some(at) = section_at.take() { doc.containers.push(Container { path: section.clone(), at, inline: false, comma: false }); }
            let header_start = ts[i].start;
            // Array tables elsewhere are retained, but never treated as a font table.
            let array = ts.get(i + 1).map(|t| t.text.as_str()) == Some("[");
            i += if array { 2 } else { 1 };
            section = keys(&ts, &mut i, "]")?;
            if section.first().map(String::as_str) == Some("font") {
                if !tables.insert(section.clone()) || doc.values.iter().any(|(path, _, _)| section.starts_with(path)) { return Err(error()); }
            }
            if array { if section.first().map(String::as_str) == Some("font") || ts.get(i).map(|t| t.text.as_str()) != Some("]") { return Err(error()); } i += 1; }
            if ts.get(i).map(|t| t.text.as_str()).filter(|s| *s != "\n").is_some() { return Err(error()); }
            // A section's insertion point is its end, set when the next header arrives.
            if let Some(last) = doc.containers.last_mut().filter(|c| !c.inline) { last.at = header_start; }
            section_at = Some(text.len());
        } else {
            let mut path = section.clone();
            path.extend(keys(&ts, &mut i, "=")?);
            value(&ts, &mut i, path, &mut doc, 0)?;
            if ts.get(i).map(|t| t.text.as_str()).filter(|s| *s != "\n").is_some() { return Err(error()); }
        }
    }
    if let Some(at) = section_at { doc.containers.push(Container { path: section, at, inline: false, comma: false }); }
    let mut seen = HashSet::new();
    for (path, _, _) in &doc.values {
        if path.first().map(String::as_str) == Some("font") && !seen.insert(path.clone()) { return Err(error()); }
    }
    Ok(doc)
}
fn nested(keys: &[String], quoted: &str) -> String {
    if keys.len() == 1 { format!("{} = {quoted}", keys[0]) }
    else { format!("{} = {{ {} }}", keys[0], nested(&keys[1..], quoted)) }
}
pub fn edit(text: &str, family: &str) -> Result<String, String> {
    let doc = parse(text)?;
    let quoted = format!("\"{}\"", crate::json::escape(family));
    let mut edits: BTreeMap<(usize, usize), String> = BTreeMap::new();
    let mut additions: BTreeMap<usize, (bool, bool, Vec<String>)> = BTreeMap::new();
    let mut appended = String::new();
    for style in ["normal", "bold", "italic", "bold_italic"] {
        let path: Vec<String> = ["font", style, "family"].iter().map(|s| s.to_string()).collect();
        if let Some((_, start, end)) = doc.values.iter().find(|(p, _, _)| p == &path) {
            edits.insert((*start, *end), quoted.clone()); continue;
        }
        let container = doc.containers.iter().filter(|c| path.starts_with(&c.path) && c.path.first().map(String::as_str) == Some("font"))
            .max_by_key(|c| c.path.len());
        let prefix = container.map(|c| c.path.len()).unwrap_or(0);
        if doc.values.iter().any(|(p, _, _)| path.starts_with(p) && p.len() > prefix) { return Err(error()); }
        if let Some(container) = container {
            let entry = additions.entry(container.at).or_insert((container.inline, container.comma, Vec::new()));
            entry.2.push(nested(&path[prefix..], &quoted));
        } else { appended.push_str(&format!("\n[font.{style}]\nfamily = {quoted}\n")); }
    }
    for (at, (inline, comma, fields)) in additions {
        let insert = if inline { format!("{}{}", if comma { ", " } else { "" }, fields.join(", ")) }
            else { format!("\n{}\n", fields.join("\n")) };
        edits.insert((at, at), insert);
    }
    let mut out = text.to_string();
    for ((start, end), value) in edits.into_iter().rev() { out.replace_range(start..end, &value); }
    out.push_str(&appended);
    // Reparse to refuse duplicate family paths or an insertion outside a table boundary.
    let checked = parse(&out)?;
    for style in ["normal", "bold", "italic", "bold_italic"] {
        let wanted: Vec<String> = ["font", style, "family"].iter().map(|s| s.to_string()).collect();
        if !checked.values.iter().any(|(p, start, end)| p == &wanted && out[*start..*end] == quoted) { return Err(error()); }
    }
    Ok(out)
}
