//! Wire adapters keep the optional denied-directory mode and captured row numbering intact.
use super::{parse_reply, Event};
use crate::pane::requests::RowAction;
use flea::{jsondoc, protocol::Reply};
use std::io::{self, Write};

pub(super) fn parse(line: &str) -> Result<Event, String> {
    match parse_reply(line)? {
        Reply::Error {
            operation,
            path,
            message,
        } => {
            let document = jsondoc::parse(line)?;
            let mode = match document.get("mode") {
                None => 0,
                Some(jsondoc::Json::Num(value)) => {
                    value.parse().map_err(|_| "mode must fit a mode")?
                }
                _ => return Err("mode must be an unsigned integer".into()),
            };
            Ok(Event::Refused {
                operation,
                path,
                message,
                mode,
            })
        }
        reply => {
            // Canonical plain windows have no gen; escaped keys still take the structural parser.
            if !line.contains("\"gen\"") && !line.contains("\\u") {
                return Ok(Event::Reply(reply));
            }
            let document = jsondoc::parse(line)?;
            let generation = match document.get("gen") {
                None => 0,
                Some(jsondoc::Json::Num(value)) => value
                    .parse()
                    .map_err(|_| "gen must be an unsigned integer")?,
                _ => return Err("gen must be an unsigned integer".into()),
            };
            if generation == 0 {
                Ok(Event::Reply(reply))
            } else {
                Ok(Event::Session { generation, reply })
            }
        }
    }
}

pub(super) fn write_rows(
    output: &mut impl Write,
    command: &str,
    action: &RowAction,
) -> io::Result<()> {
    write!(
        output,
        "{{\"c\":\"{command}\",\"listing\":{},\"rows\":[",
        action.listing
    )?;
    for (at, row) in action.rows.iter().enumerate() {
        if at > 0 {
            write!(output, ",")?;
        }
        write!(output, "{row}")?;
    }
    writeln!(output, "]}}")?;
    output.flush()
}
