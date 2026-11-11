//! The listing subset of the backend's JSON-lines protocol.

use crate::json::escape;
use crate::jsondoc::{self, Json};
use std::io::{self, Write};

#[derive(Debug, PartialEq)]
pub enum Request<'a> {
    List {
        path: &'a str,
        first: usize,
        select: Option<&'a str>,
    },
    Window {
        start: usize,
        count: usize,
    },
    Quit,
}

impl Request<'_> {
    /// Writes and flushes exactly one request line, escaping paths as data.
    pub fn write_to(&self, output: &mut impl Write) -> io::Result<()> {
        match self {
            Self::List {
                path,
                first,
                select,
            } => {
                write!(
                    output,
                    "{{\"c\":\"list\",\"path\":\"{}\",\"first\":{first},\"hidden\":false",
                    escape(path)
                )?;
                if let Some(select) = select {
                    write!(output, ",\"select\":\"{}\"", escape(select))?;
                }
                writeln!(output, "}}")?;
            }
            Self::Window { start, count } => writeln!(
                output,
                "{{\"c\":\"window\",\"start\":{start},\"count\":{count}}}"
            )?,
            Self::Quit => writeln!(output, "{{\"c\":\"quit\"}}")?,
        }
        output.flush()
    }
}

/// The fields of one `rows` entry a list row draws, read from docs/protocol.md "rows".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowFacts {
    /// The lstat identity of this displayed source, carried into inline rename.
    pub source: Option<String>,
    pub depth: usize,
    pub open: bool,
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub mtime: i64,
    pub mode: u32,
    pub icon: String,
    pub link: Option<String>,
    /// False only when the backend sends `"x":false`: the mount has no real modes, so `mode` marks no executable.
    pub real_modes: bool,
    /// True only when the backend sends `"t":false`: no thumbnailer can picture this row, so no view asks for one.
    pub thumbless: bool,
    /// The row's Kind description, `kinds[k]` of its reply; empty when the reply names none.
    pub kind: String,
    /// The folder size the live answer walked inline for this row, from its `z` pairs; None when it named none.
    pub inline: Option<u64>,
    /// The optional tg field; absent rows do not latch a folder's tag slot.
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, PartialEq)]
pub enum Reply {
    Listed {
        path: String,
        total: usize,
    },
    Rows {
        start: usize,
        rows: Vec<RowFacts>,
        listing: usize,
    },
    Error {
        operation: String,
        path: String,
        message: String,
        mode: u32,
    },
    Other,
}

/// Reads fields from their own object, so nested row keys cannot impersonate the envelope.
/// Sample input: {"t":"listed","path":"/files","n":1}
/// Sample input: {"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt"}]}
/// Sample input: {"t":"error","where":"window","path":"/files","msg":"refused"}
pub fn parse_reply(line: &str) -> Result<Reply, String> {
    let document = jsondoc::parse(line)?;
    match string(&document, "t")? {
        "listed" => Ok(Reply::Listed {
            path: string(&document, "path")?.to_owned(),
            total: uint(&document, "n")?,
        }),
        "rows" => {
            let rows = document
                .get("rows")
                .and_then(Json::as_array)
                .ok_or("rows must be an array")?;
            let kinds = document
                .get("kinds")
                .and_then(Json::as_array)
                .unwrap_or(&[]);
            let start = uint(&document, "start")?;
            let mut rows: Vec<RowFacts> = rows
                .iter()
                .map(|row| row_facts(row, kinds))
                .collect::<Result<_, _>>()?;
            // Each z pair is [absolute row, bytes]: a folder size the backend walked inline with this answer.
            for pair in document.get("z").and_then(Json::as_array).unwrap_or(&[]) {
                let (row, bytes): (Option<usize>, Option<u64>) = match pair.as_array() {
                    Some([row, bytes]) => (number(row), number(bytes)),
                    _ => (None, None),
                };
                let (Some(row), Some(bytes)) = (row, bytes) else {
                    return Err("z must hold [row,bytes] pairs".into());
                };
                if let Some(facts) = row.checked_sub(start).and_then(|at| rows.get_mut(at)) {
                    facts.inline = Some(bytes);
                }
            }
            Ok(Reply::Rows {
                start,
                rows,
                listing: uint(&document, "listing")?,
            })
        }
        "error" => Ok(Reply::Error {
            operation: string(&document, "where")?.to_owned(),
            path: string(&document, "path")?.to_owned(),
            message: string(&document, "msg")?.to_owned(),
            mode: match document.get("mode") {
                None => 0,
                Some(Json::Num(value)) => value.parse().map_err(|_| "mode must fit a mode")?,
                _ => return Err("mode must be an unsigned integer".into()),
            },
        }),
        _ => Ok(Reply::Other),
    }
}

fn row_facts(row: &Json, kinds: &[Json]) -> Result<RowFacts, String> {
    let link = match row.get("l") {
        None => None,
        Some(_) => Some(string(row, "l")?.to_owned()),
    };
    let real_modes = match row.get("x") {
        None => true,
        Some(value) => value.as_bool().ok_or("x must be a boolean")?,
    };
    Ok(RowFacts {
        source: row.get("source").and_then(Json::as_str).map(str::to_owned),
        depth: row
            .get("depth")
            .map(|_| uint(row, "depth"))
            .transpose()?
            .unwrap_or(0),
        open: row.get("open").and_then(Json::as_bool).unwrap_or(false),
        tags: row
            .get("tg")
            .map(|value| {
                value
                    .as_array()
                    .ok_or_else(|| "tg must be an array".to_owned())?
                    .iter()
                    .map(|tag| {
                        tag.as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| "tg names must be strings".to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?,
        name: string(row, "n")?.to_owned(),
        dir: row
            .get("d")
            .and_then(Json::as_bool)
            .ok_or("d must be a boolean")?,
        size: uint(row, "s")? as u64,
        mtime: int(row, "m")?,
        mode: u32::try_from(uint(row, "p")?).map_err(|_| "p must fit a mode")?,
        icon: string(row, "i")?.to_owned(),
        link,
        real_modes,
        thumbless: row.get("t").and_then(Json::as_bool) == Some(false),
        // k is an index into kinds; anything but an unsigned integer in range names no kind, as uint() reads one.
        kind: match row.get("k") {
            Some(Json::Num(k)) => k.parse::<usize>().ok(),
            _ => None,
        }
        .and_then(|k| kinds.get(k))
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_owned(),
        inline: None,
    })
}

fn number<T: std::str::FromStr>(value: &Json) -> Option<T> {
    match value {
        Json::Num(number) => number.parse().ok(),
        _ => None,
    }
}

fn string<'a>(value: &'a Json, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{key} must be a string"))
}

fn uint(value: &Json, key: &str) -> Result<usize, String> {
    match value.get(key) {
        Some(Json::Num(number)) => number
            .parse()
            .map_err(|_| format!("{key} must be an unsigned integer")),
        _ => Err(format!("{key} must be an unsigned integer")),
    }
}

fn int(value: &Json, key: &str) -> Result<i64, String> {
    match value.get(key) {
        Some(Json::Num(number)) => number
            .parse()
            .map_err(|_| format!("{key} must be an integer")),
        _ => Err(format!("{key} must be an integer")),
    }
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
