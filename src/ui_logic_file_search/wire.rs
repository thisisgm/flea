// The session's requests and its own reply lines, in the shapes docs/protocol.md "searchopen" to "searchclose" names.
use crate::json::escape;
use crate::jsondoc::{self, Json};

// Producers a front end can ask for; at this base only the folder and Home scopes walk, and only by name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wire {
    pub everywhere: bool,
    pub contents: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Contents { path: String, query: String, hidden: bool, gen: u64, count: usize },
    Open { scope: String, folder: String, hidden: bool },
    Query { query: String, gen: u64, count: usize, sizes: usize },
    Cancel,
    Close,
}

impl Request {
    // Sample output: {"c":"searchquery","query":"not","gen":3,"count":40,"sizes":0}
    pub fn line(&self) -> String {
        match self {
            Self::Contents { path, query, hidden, gen, count } => format!(
                r#"{{"c":"searchcontents","path":"{}","query":"{}","hidden":{},"gen":{},"count":{}}}"#,
                escape(path), escape(query), hidden, gen, count
            ),
            Self::Open { scope, folder, hidden } => format!(
                r#"{{"c":"searchopen","path":"{}","folder":"{}","hidden":{}}}"#,
                escape(scope),
                escape(folder),
                hidden
            ),
            Self::Query { query, gen, count, sizes } => format!(
                r#"{{"c":"searchquery","query":"{}","gen":{},"count":{},"sizes":{}}}"#,
                escape(query),
                gen,
                count,
                sizes
            ),
            Self::Cancel => r#"{"c":"searchcancel"}"#.to_owned(),
            Self::Close => r#"{"c":"searchclose"}"#.to_owned(),
        }
    }
}

// The lines only a search writes; listed and rows stay the listing's own and carry the gen beside them.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    ContentsMatches(super::contents::ContentsMatches),
    // partial names the producer's reason when its index was incomplete, None for a complete one.
    Searched { n: usize, scanned: usize, ms: f64, cancelled: bool, gen: u64, walking: bool, partial: Option<String> },
    Searching { n: usize, scanned: usize, ms: f64 },
    Opened { path: String },
    Closed { path: String },
}

fn uint(value: &Json, key: &str) -> Result<usize, String> {
    match value.get(key) {
        Some(Json::Num(number)) => number.parse().map_err(|_| format!("{key} must be an unsigned integer")),
        _ => Err(format!("{key} must be an unsigned integer")),
    }
}

fn float(value: &Json, key: &str) -> Result<f64, String> {
    value.get(key).and_then(Json::as_f64).filter(|ms| ms.is_finite() && *ms >= 0.).ok_or_else(|| format!("{key} must be a number"))
}

fn flag(value: &Json, key: &str) -> Result<bool, String> {
    match value.get(key) {
        None => Ok(false),
        Some(found) => found.as_bool().ok_or_else(|| format!("{key} must be a boolean")),
    }
}

fn text(value: &Json, key: &str) -> Result<String, String> {
    value.get(key).and_then(Json::as_str).map(str::to_owned).ok_or_else(|| format!("{key} must be a string"))
}

// A partial answer must say why; an older producer's line without the field is a complete one.
fn partial(value: &Json) -> Result<Option<String>, String> {
    if !flag(value, "partial")? {
        return Ok(None);
    }
    match value.get("partial_reason").and_then(Json::as_str) {
        Some(reason) if !reason.is_empty() => Ok(Some(reason.to_owned())),
        _ => Err("a partial answer needs a partial_reason".into()),
    }
}

// None for a line that is not a search's own; a malformed search line is an error, never a guess.
// Sample input: {"t":"searched","n":11,"scanned":1040,"ms":12.400,"cancelled":false,"gen":7,"walking":false}
pub fn parse_line(line: &str) -> Result<Option<Line>, String> {
    let document = jsondoc::parse(line)?;
    let kind = match document.get("t").and_then(Json::as_str) {
        Some(kind) => kind,
        None => return Err("t must be a string".into()),
    };
    Ok(Some(match kind {
        "contentsmatches" => Line::ContentsMatches(super::contents::ContentsMatches::parse(&document)?),
        "searched" => Line::Searched {
            n: uint(&document, "n")?,
            scanned: uint(&document, "scanned")?,
            ms: float(&document, "ms")?,
            cancelled: flag(&document, "cancelled")?,
            // A plain search's walk carries no gen, and gen 0 is never a session's.
            gen: match document.get("gen") {
                None => 0,
                Some(_) => uint(&document, "gen")? as u64,
            },
            walking: flag(&document, "walking")?,
            partial: partial(&document)?,
        },
        "searching" => Line::Searching {
            n: uint(&document, "n")?,
            scanned: uint(&document, "scanned")?,
            ms: float(&document, "ms")?,
        },
        "searchopened" => Line::Opened { path: text(&document, "path")? },
        "searchclosed" => Line::Closed { path: text(&document, "path")? },
        _ => return Ok(None),
    }))
}
