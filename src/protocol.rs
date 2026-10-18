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
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub mtime: i64,
    pub mode: u32,
    pub icon: String,
    pub link: Option<String>,
    /// False only when the backend sends `"x":false`: the mount has no real modes, so `mode` marks no executable.
    pub real_modes: bool,
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
            Ok(Reply::Rows {
                start: uint(&document, "start")?,
                rows: rows.iter().map(row_facts).collect::<Result<_, _>>()?,
                listing: uint(&document, "listing")?,
            })
        }
        "error" => Ok(Reply::Error {
            operation: string(&document, "where")?.to_owned(),
            path: string(&document, "path")?.to_owned(),
            message: string(&document, "msg")?.to_owned(),
        }),
        _ => Ok(Reply::Other),
    }
}

fn row_facts(row: &Json) -> Result<RowFacts, String> {
    let link = match row.get("l") {
        None => None,
        Some(_) => Some(string(row, "l")?.to_owned()),
    };
    let real_modes = match row.get("x") {
        None => true,
        Some(value) => value.as_bool().ok_or("x must be a boolean")?,
    };
    Ok(RowFacts {
        name: string(row, "n")?.to_owned(),
        dir: row.get("d").and_then(Json::as_bool).ok_or("d must be a boolean")?,
        size: uint(row, "s")? as u64,
        mtime: int(row, "m")?,
        mode: u32::try_from(uint(row, "p")?).map_err(|_| "p must fit a mode")?,
        icon: string(row, "i")?.to_owned(),
        link,
        real_modes,
    })
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
