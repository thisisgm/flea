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

#[derive(Debug, PartialEq)]
pub enum Reply {
    Listed {
        path: String,
        total: usize,
    },
    Rows {
        start: usize,
        names: Vec<String>,
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
            let names = rows
                .iter()
                .map(|row| string(row, "n").map(str::to_owned))
                .collect::<Result<_, _>>()?;
            Ok(Reply::Rows {
                start: uint(&document, "start")?,
                names,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_paths_cannot_inject_another_line_or_field() {
        let path = "/files/a\"b\nnext\\file";
        let mut wire = Vec::new();
        Request::List {
            path,
            first: 20,
            select: Some(path),
        }
        .write_to(&mut wire)
        .unwrap();
        let text = String::from_utf8(wire).unwrap();
        assert_eq!(text.lines().count(), 1);
        let document = jsondoc::parse(&text).unwrap();
        assert_eq!(string(&document, "c").unwrap(), "list");
        assert_eq!(string(&document, "path").unwrap(), path);
        assert_eq!(string(&document, "select").unwrap(), path);
        assert_eq!(uint(&document, "first").unwrap(), 20);
        assert_eq!(document.get("hidden"), Some(&Json::Bool(false)));
        let mut wire = Vec::new();
        Request::List {
            path,
            first: 0,
            select: None,
        }
        .write_to(&mut wire)
        .unwrap();
        assert!(jsondoc::parse(std::str::from_utf8(&wire).unwrap())
            .unwrap()
            .get("select")
            .is_none());
    }

    #[test]
    fn window_and_quit_keep_the_existing_wire_shape() {
        let mut wire = Vec::new();
        Request::Window {
            start: 17,
            count: 20,
        }
        .write_to(&mut wire)
        .unwrap();
        Request::Quit.write_to(&mut wire).unwrap();
        assert_eq!(
            wire,
            b"{\"c\":\"window\",\"start\":17,\"count\":20}\n{\"c\":\"quit\"}\n"
        );
    }

    #[test]
    fn parses_the_initial_pair_and_preserves_escaped_names() {
        assert_eq!(
            parse_reply(r#"{"t":"listed","n":2,"path":"/files/"}"#).unwrap(),
            Reply::Listed {
                path: "/files/".into(),
                total: 2
            }
        );
        assert_eq!(parse_reply(r#"{"t":"rows","start":42,"rows":[{"n":"a\nb","t":false},{"n":"caf\u00e9","t":true}],"listing":9007199254740993}"#).unwrap(),
            Reply::Rows { start: 42, names: vec!["a\nb".into(), "caf\u{e9}".into()], listing: 9007199254740993 });
        assert_eq!(
            parse_reply(r#"{"t":"rows","start":0,"rows":[],"listing":1}"#).unwrap(),
            Reply::Rows {
                start: 0,
                names: vec![],
                listing: 1
            }
        );
    }

    #[test]
    fn rejects_malformed_fields_and_never_uses_nested_envelope_fields() {
        for bad in [
            r#"{"rows":[{"t":"listed","n":2,"path":"/wrong"}]}"#,
            r#"{"t":"listed","n":-1,"path":"/files"}"#,
            r#"{"t":"listed","n":1.5,"path":"/files"}"#,
            r#"{"t":"rows","start":0,"rows":[{"n":4}],"listing":1}"#,
            r#"{"t":"rows"}"#,
            "{",
        ] {
            assert!(parse_reply(bad).is_err(), "{bad}");
        }
        assert_eq!(
            parse_reply(r#"{"t":"changed","path":"/files"}"#).unwrap(),
            Reply::Other
        );
        assert_eq!(
            parse_reply(r#"{"t":"error","where":"scan","path":"/files","msg":"denied"}"#).unwrap(),
            Reply::Error {
                operation: "scan".into(),
                path: "/files".into(),
                message: "denied".into()
            }
        );
    }

    #[test]
    fn a_write_failure_is_returned_to_the_caller() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            Request::Quit.write_to(&mut Closed).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}
