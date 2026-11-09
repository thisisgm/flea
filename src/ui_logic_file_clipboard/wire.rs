//! The clipboard's backend lines: what the window sends and what a t:clip reply carries, parsed once into events; shapes come from src/clip/reply.rs and src/backend/clipreq.rs, and the window never builds these by hand.
use crate::json::{escape, string_array};
use crate::jsondoc::{self, Json};

// Sample input: cut over ["/a/f2"] sends {"c":"clipSet","op":"cut","paths":["/a/f2"]}.
pub fn clip_set_line(op: &str, paths: &[String]) -> String {
    format!(
        "{{\"c\":\"clipSet\",\"op\":\"{}\",\"paths\":{}}}",
        escape(op),
        string_array(paths)
    )
}

pub fn clip_get_line() -> String {
    "{\"c\":\"clipGet\"}".to_string()
}

// Sample input: a spent owned cut clears by its token, never by its paths.
pub fn clip_clear_line(token: &str, cut: &[String]) -> String {
    if token.is_empty() {
        format!("{{\"c\":\"clipClear\",\"cut\":{}}}", string_array(cut))
    } else {
        format!("{{\"c\":\"clipClear\",\"token\":\"{}\"}}", escape(token))
    }
}

pub fn clip_watch_line() -> String {
    "{\"c\":\"clipWatch\"}".to_string()
}

pub fn paths_request_line(rows: &[usize]) -> String {
    let rows = rows.iter().map(|row| row.to_string()).collect::<Vec<_>>().join(",");
    format!("{{\"c\":\"paths\",\"rows\":[{rows}]}}")
}

// Sample output: {"t":"paths","paths":["/home/gm/a.txt","/home/gm/b.txt"]}.
pub fn parse_paths(line: &str) -> Result<Option<Vec<String>>, String> {
    if !line.contains(r#""t":"paths""#) {
        return Ok(None);
    }
    let doc = jsondoc::parse(line)?;
    paths(&doc).map(Some)
}

pub fn paste_image_line(dest: &str, name: &str) -> String {
    format!(
        "{{\"c\":\"pasteimage\",\"dest\":\"{}\",\"name\":\"{}\"}}",
        escape(dest),
        escape(name)
    )
}

/// One parsed `t:clip` line; every other line is None, never an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipEvent {
    Set { ok: bool, token: String, error: String },
    Changed { clip: String, paths: Vec<String>, token: String, error: String },
    Get { ok: bool, clip: String, paths: Vec<String>, token: String, error: String },
    Clear { ok: bool, error: String },
}

fn text(doc: &Json, key: &str) -> Result<String, String> {
    match doc.get(key) {
        None => Ok(String::new()),
        Some(Json::Str(value)) => Ok(value.clone()),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn paths(doc: &Json) -> Result<Vec<String>, String> {
    match doc.get("paths") {
        None => Ok(Vec::new()),
        Some(Json::Arr(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "paths must be strings".to_string())
            })
            .collect::<Result<_, _>>(),
        Some(_) => Err("paths must be an array".into()),
    }
}

// Sample input: {"t":"clip","op":"set","ok":true,"token":"own"} is Set; {"t":"job"} is None.
pub fn parse_clip(line: &str) -> Result<Option<ClipEvent>, String> {
    if !line.contains(r#""t":"clip""#) {
        return Ok(None);
    }
    let doc = jsondoc::parse(line)?;
    let op = doc.get("op").and_then(Json::as_str).unwrap_or("");
    let ok = doc.get("ok").and_then(Json::as_bool).unwrap_or(false);
    Ok(Some(match op {
        "set" => ClipEvent::Set {
            ok,
            token: text(&doc, "token")?,
            error: text(&doc, "error")?,
        },
        "changed" => ClipEvent::Changed {
            clip: text(&doc, "clip")?,
            paths: paths(&doc)?,
            token: text(&doc, "token")?,
            error: text(&doc, "error")?,
        },
        "get" => ClipEvent::Get {
            ok,
            clip: text(&doc, "clip")?,
            paths: paths(&doc)?,
            token: text(&doc, "token")?,
            error: text(&doc, "error")?,
        },
        "clear" => ClipEvent::Clear {
            ok,
            error: text(&doc, "error")?,
        },
        _ => return Err(format!("a clip line names no known op: {op}")),
    }))
}
