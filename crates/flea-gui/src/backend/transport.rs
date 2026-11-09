//! Wire adapters keep the optional denied-directory mode and captured row numbering intact.
use super::{Event, Located};
use crate::navigation::{folder_state::Order, Command, Listing};
use crate::pane::requests::RowAction;
use flea::{json::escape, jsondoc, protocol::parse_reply, protocol::Reply};
use std::io::{self, Write};

pub(super) fn parse(line: &str) -> Result<Event, String> {
    match parse_reply(line)? {
        // A contents search failure keeps its gen, so the search settles only the query it answers.
        Reply::Error { operation, .. } if operation == "searchcontents" => {
            Ok(Event::Producer(line.to_owned()))
        }
        Reply::Error {
            operation,
            path,
            message,
            mode,
        } => Ok(Event::Refused {
            operation,
            path,
            message,
            mode,
        }),
        Reply::Other => Ok(other(line)?.unwrap_or_else(|| {
            if flea::ui_logic_file_ops::parse(line).is_some()
                && flea::jsondoc::parse(line)
                    .ok()
                    .and_then(|doc| {
                        doc.get("t")
                            .and_then(flea::jsondoc::Json::as_str)
                            .map(str::to_owned)
                    })
                    .as_deref()
                    != Some("menuaction")
            {
                Event::Operations(line.into())
            } else {
                Event::Producer(line.to_owned())
            }
        })),
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

// The changed count a wantChanged re-list carries on its listed line.
pub(super) fn reload_changed(event: &Event, line: &str) -> Option<usize> {
    match event {
        Event::Reply(Reply::Listed { .. }) => flea::json::field_usize(line, "changed"),
        _ => None,
    }
}

// A listed line's folder and its "w"; a line without "w" reads writable, as Backend.qml defaults.
pub(super) fn listed_folder(event: &Event, line: &str) -> Option<(String, bool)> {
    let (Event::Reply(Reply::Listed { path, .. })
    | Event::Session {
        reply: Reply::Listed { path, .. },
        ..
    }) = event
    else {
        return None;
    };
    let writable = jsondoc::parse(line)
        .ok()?
        .get("w")
        .and_then(jsondoc::Json::as_bool)
        != Some(false);
    Some((path.clone(), writable))
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

// Unsolicited and pathed replies the listing parser has no variant for.
pub(super) fn other(line: &str) -> Result<Option<Event>, String> {
    let document = jsondoc::parse(line)?;
    // A figures-bearing fsinfo is a status fact even without a class; class-only lines go to sizes.
    if document.get("t").and_then(jsondoc::Json::as_str) == Some("fsinfo")
        && (document.get("fs").is_some() || document.get("free").is_some())
    {
        let facts = super::status::Filesystem::parse(&document)?;
        let reply = match document.get("class") {
            Some(class) => Event::FilesystemDetails {
                facts,
                class: class
                    .as_str()
                    .map(str::to_owned)
                    .ok_or("class must be a string")?,
            },
            None => Event::Filesystem(facts),
        };
        return Ok(Some(match document.get("system") {
            Some(system) => Event::FilesystemSystem {
                system: system.as_bool().ok_or("system must be a boolean")?,
                reply: Box::new(reply),
            },
            None => reply,
        }));
    }
    if let Some(reply) = crate::list::sizes::parse(&document)? {
        return Ok(Some(match reply {
            crate::list::sizes::Reply::Filesystem { class, .. }
                if document.get("fs").is_some() || document.get("free").is_some() =>
            {
                Event::FilesystemDetails {
                    facts: super::status::Filesystem::parse(&document)?,
                    class,
                }
            }
            reply => Event::ListDetails(reply),
        }));
    }
    // Preserve every peek field for the path-to-Columns claim chain in Shell::receive.
    if document.get("t").and_then(jsondoc::Json::as_str) == Some("peeked") {
        return Ok(Some(Event::Peeked(document)));
    }
    if let Some(answer) = crate::path::answer(&document) {
        return Ok(Some(Event::Path(answer)));
    }
    let text = |key| match document.get(key) {
        Some(jsondoc::Json::Str(value)) => Ok(value.clone()),
        _ => Err(format!("{key} must be a string")),
    };
    let number = |value: Option<&jsondoc::Json>| match value {
        Some(jsondoc::Json::Num(value)) => value.parse::<i64>().ok(),
        _ => None,
    };
    Ok(match document.get("t") {
        Some(jsondoc::Json::Str(kind)) if kind == "fsinfo" => Some(Event::Filesystem(
            super::status::Filesystem::parse(&document)?,
        )),
        Some(jsondoc::Json::Str(kind))
            if matches!(
                kind.as_str(),
                "plugins" | "pluginop" | "pluginruntime" | "pluginprogress"
            ) =>
        {
            Some(Event::Plugins(line.into()))
        }
        Some(jsondoc::Json::Str(kind)) if kind == "peeked" => Some(Event::Peeked(document)),
        Some(jsondoc::Json::Str(kind))
            if matches!(
                kind.as_str(),
                "trashbrowse" | "trashthumbed" | "trashdeletestarted"
            ) =>
        {
            Some(Event::TrashLine(line.to_owned()))
        }
        Some(jsondoc::Json::Str(kind)) if kind == "changed" => Some(Event::Changed {
            path: text("path")?,
        }),
        Some(jsondoc::Json::Str(kind)) if kind == "paths" => Some(Event::Paths(
            document
                .get("paths")
                .and_then(jsondoc::Json::as_array)
                .ok_or("paths must be an array")?
                .iter()
                .map(|path| {
                    path.as_str()
                        .map(str::to_owned)
                        .ok_or("paths must be strings")
                })
                .collect::<Result<_, _>>()?,
        )),
        Some(jsondoc::Json::Str(kind)) if kind == "located" => {
            let matches = match document.get("matches").and_then(jsondoc::Json::as_array) {
                Some(found) => found
                    .iter()
                    .filter_map(|found| {
                        let index = usize::try_from(number(found.get("index"))?).ok()?;
                        Some((found.get("path")?.as_str()?.to_owned(), index))
                    })
                    .collect(),
                None => Vec::new(),
            };
            Some(Event::Located(Located {
                directory: text("directory")?,
                path: text("path").unwrap_or_default(),
                index: number(document.get("index")).unwrap_or(-1),
                id: number(document.get("id")).and_then(|id| u64::try_from(id).ok()),
                transfer: number(document.get("transferId")).unwrap_or(0),
                ok: document.get("ok").and_then(jsondoc::Json::as_bool) != Some(false),
                matches,
            }))
        }
        Some(jsondoc::Json::Str(kind)) if kind == "peeked" || kind == "jumped" => Some(
            Event::Path(crate::path::answer(&document).ok_or("path answer is malformed")?),
        ),
        _ => None,
    })
}

fn order_fields(
    order: Order,
    folders_first: bool,
    group_by_kind: bool,
    hidden_last: bool,
) -> String {
    format!(
        "\"by\":\"{}\",\"desc\":{},\"foldersFirst\":{folders_first},\"groupByKind\":{group_by_kind},\"hiddenLast\":{hidden_last}",
        order.key, order.desc
    )
}

// The one list composition: the folder's order and filters ride every request.
pub fn list_line(listing: &Listing) -> String {
    let mut line = format!(
        "{{\"c\":\"list\",\"path\":\"{}\",\"first\":{},\"hidden\":{},\"wantChanged\":{},{}",
        escape(&listing.path),
        listing.first,
        listing.hidden,
        listing.want_changed,
        order_fields(
            listing.order,
            listing.folders_first,
            listing.group_by_kind,
            listing.hidden_last
        )
    );
    if let Some(select) = &listing.select {
        line.push_str(&format!(",\"select\":\"{}\"", escape(select)));
    }
    line.push('}');
    line
}

pub(super) fn command_line(command: &Command) -> Option<String> {
    Some(match command {
        Command::List(listing) => list_line(listing),
        Command::ListPaths(listing, paths) => {
            let paths = paths
                .iter()
                .map(|path| format!("\"{}\"", escape(path)))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"c\":\"listpaths\",\"paths\":[{paths}],\"first\":{}}}",
                listing.first
            )
        }
        Command::Window { start, count } => {
            format!("{{\"c\":\"window\",\"start\":{start},\"count\":{count}}}")
        }
        Command::Sort {
            order,
            folders_first,
            group_by_kind,
            hidden_last,
        } => format!(
            "{{\"c\":\"sort\",{}}}",
            order_fields(*order, *folders_first, *group_by_kind, *hidden_last)
        ),
        Command::Locate { path } => format!("{{\"c\":\"locate\",\"path\":\"{}\"}}", escape(path)),
        Command::LocateMany { paths, id } => {
            let paths: Vec<String> = paths
                .iter()
                .map(|path| format!("\"{}\"", escape(path)))
                .collect();
            format!(
                "{{\"c\":\"locate\",\"paths\":[{}],\"id\":{id}}}",
                paths.join(",")
            )
        }
        Command::Paths(_) | Command::Persist(_) => return None,
    })
}

#[cfg(test)]
mod tab_peek_tests {
    use super::*;

    #[test]
    fn rev_u25_a_columns_peek_is_not_claimed_as_a_tab_peek() {
        for first in [2, 320] {
            let line = format!(
                "{{\"t\":\"peeked\",\"path\":\"/files/child\",\"first\":{first},\"n\":1,\"rows\":[{{\"n\":\"seed.txt\",\"d\":false}}],\"hidden\":false,\"hiddenLast\":false}}"
            );
            let Event::Peeked(document) = parse(&line).unwrap() else {
                panic!("every peek must keep its shared event shape");
            };
            assert_eq!(document, jsondoc::parse(&line).unwrap());
            // The same parsed event reaches the strip, which claims only its reserved folder check.
            let peek = crate::tabs::transfer::peeked(&document).unwrap();
            let mut world = crate::tabs::test_support::World::new(1);
            let mut bar = world.bar();
            let payload =
                crate::tabs::transfer::payload("222", "pending", "/files/child", "list", "");
            assert!(bar.strip_drop(&mut world, &payload, 0., 1));
            bar.on_peeked(
                &mut world,
                0,
                &peek.path,
                peek.hidden,
                peek.read_failed,
                peek.hidden_last,
                peek.first,
            );
            assert_eq!(
                bar.pending.is_some(),
                first != crate::tabs::transfer::PEEK_FIRST,
                "first={first}"
            );
        }
    }

    #[test]
    fn ey65_fsinfo_figures_are_a_status_fact_with_or_without_a_class() {
        let bare = parse(r#"{"t":"fsinfo","path":"/a","fs":"btrfs","free":5}"#).unwrap();
        assert!(
            matches!(bare, Event::Filesystem(facts) if facts.name == "btrfs" && facts.free == 5)
        );
        let classed =
            parse(r#"{"t":"fsinfo","path":"/a","fs":"","free":-1,"class":"ssd"}"#).unwrap();
        assert!(
            matches!(classed, Event::FilesystemDetails { facts, class } if facts.name.is_empty() && class == "ssd")
        );
    }
}
