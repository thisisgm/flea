//! Contents metadata shared by the GUI and TUI; display text never names an operation target.
use crate::json::escape;
use crate::jsondoc::Json;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentsMatch {
    pub row: usize,
    pub path: String,
    pub path_bytes: Vec<u8>,
    pub line_no: usize,
    pub line: String,
    pub operable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentsMatches {
    pub gen: u64,
    pub listing: u64,
    pub start: usize,
    pub matches: Vec<ContentsMatch>,
}

pub fn contents_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 {
        return Err("pathBytes must contain hex pairs".into());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |b: u8| {
                (b as char)
                    .to_digit(16)
                    .ok_or_else(|| "pathBytes must be hexadecimal".to_owned())
            };
            Ok((digit(pair[0])? * 16 + digit(pair[1])?) as u8)
        })
        .collect()
}

fn uint(value: &Json, key: &str) -> Result<usize, String> {
    match value.get(key) {
        Some(Json::Num(n)) => n
            .parse()
            .map_err(|_| format!("{key} must be an unsigned integer")),
        _ => Err(format!("{key} must be an unsigned integer")),
    }
}

fn text(value: &Json, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Json::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{key} must be a string"))
}

impl ContentsMatches {
    // Sample input: {"t":"contentsmatches","gen":1,"listing":1,"start":0,"matches":[]}
    pub(crate) fn parse(document: &Json) -> Result<Self, String> {
        let gen = uint(document, "gen")?;
        let listing = uint(document, "listing")?;
        if gen == 0 || listing == 0 {
            return Err("contents identity must be nonzero".into());
        }
        let start = uint(document, "start")?;
        let values = document
            .get("matches")
            .and_then(Json::as_array)
            .ok_or("matches must be an array")?;
        let mut matches = Vec::with_capacity(values.len());
        for (offset, value) in values.iter().enumerate() {
            let row = uint(value, "row")?;
            if start.checked_add(offset) != Some(row) {
                return Err("contents rows must be contiguous".into());
            }
            let path_bytes = unhex(&text(value, "pathBytes")?)?;
            if path_bytes.is_empty()
                || path_bytes.contains(&0)
                || path_bytes.starts_with(b"/")
                || path_bytes
                    .split(|b| *b == b'/')
                    .any(|part| part.is_empty() || part == b"." || part == b"..")
            {
                return Err("contents path must stay relative to its scope".into());
            }
            let path = text(value, "path")?;
            if String::from_utf8_lossy(&path_bytes) != path {
                return Err("contents display does not match its identity".into());
            }
            let operable = value
                .get("operable")
                .and_then(Json::as_bool)
                .ok_or("operable must be a boolean")?;
            if operable && std::str::from_utf8(&path_bytes).is_err() {
                return Err("a raw filename cannot be operable".into());
            }
            let line_no = uint(value, "lineNo")?;
            if line_no == 0 {
                return Err("lineNo must be positive".into());
            }
            matches.push(ContentsMatch {
                row,
                path,
                path_bytes,
                line_no,
                line: text(value, "line")?,
                operable,
            });
        }
        Ok(Self {
            gen: gen as u64,
            listing: listing as u64,
            start,
            matches,
        })
    }

    pub fn line(&self) -> String {
        let mut out = format!(
            r#"{{"t":"contentsmatches","gen":{},"listing":{},"start":{},"matches":["#,
            self.gen, self.listing, self.start
        );
        for (index, hit) in self.matches.iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            out.push_str(&format!(r#"{{"row":{},"path":"{}","pathBytes":"{}","lineNo":{},"line":"{}","operable":{}}}"#,
                hit.row, escape(&hit.path), contents_hex(&hit.path_bytes), hit.line_no, escape(&hit.line), hit.operable));
        }
        out.push_str("]}");
        out
    }
}
