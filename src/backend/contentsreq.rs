use super::contents::{self, Ask, Done, Outcome};
use super::events::Event;
use super::state::{Held, State, Tables};
use super::thumbs::Pool;
use crate::backend::events::Sender;
use crate::error::FleaError;
use crate::json::{escape, field_str, field_usize, field_usize_array};
use crate::jsondoc::Json;
use flea::ui_logic_file_search::ContentsMatches;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
#[path = "contentsguard.rs"]
mod guard;
#[path = "contentsorder.rs"]
mod order;

const METADATA_CHUNK_BYTES: usize = 128 * 1024;
// One text byte may become six bytes in a JSON Unicode escape.
const JSON_TEXT_ESCAPE_BYTES: usize = 6;
// The path appears as hexadecimal bytes plus display text with JSON escaping.
const RAW_PATH_ENVELOPE_BYTES: usize = 8;
const MATCH_ENVELOPE_BYTES: usize = 256;

// Contents publications keep their own worker, generation and raw identity while invalidating row indices.
pub fn forget_rows(st: &mut State, pool: &Pool) {
    let contents = std::mem::take(&mut st.contents);
    super::run::forget_rows(st, pool);
    st.contents = contents;
}

pub fn start(line: &str, out: &mut impl Write, st: &mut State, tx: &Sender<Event>) {
    match Ask::parse(line) {
        Err(reason) => error(out, None, &reason),
        Ok(ask) => {
            if !st.contents.accepts(ask.gen) {
                return;
            }
            if let Err(reason) = st.contents.submit(ask.clone(), tx) {
                error(out, Some(&ask), &reason);
            }
        }
    }
}

pub fn intercept(
    line: &str,
    request: &super::proto::Request,
    out: &mut impl Write,
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
) -> bool {
    if let Some(refused) = refusal(line, st) {
        super::run::say(out, &refused);
        return true;
    }
    match request {
        super::proto::Request::List { .. }
        | super::proto::Request::ListPaths { .. }
        | super::proto::Request::Search { .. }
        | super::proto::Request::SearchOpen { .. } => {
            st.contents.clear();
            false
        }
        super::proto::Request::Window { start, count } if st.contents.held.is_some() => {
            window(out, st, tb, *start, *count);
            true
        }
        super::proto::Request::Sort { anchor, .. } if st.contents.held.is_some() => {
            order::sort(line, anchor.as_deref(), out, st, tb, pool);
            true
        }
        super::proto::Request::SearchCancel if st.contents.busy() || st.contents.held.is_some() => {
            cancel(out, st, false);
            true
        }
        super::proto::Request::SearchClose
            if st.contents.busy() || st.contents.has_scope() || st.contents.held.is_some() =>
        {
            cancel(out, st, true);
            true
        }
        _ => false,
    }
}

pub fn error(out: &mut impl Write, ask: Option<&Ask>, reason: &str) {
    let error = FleaError {
        where_: "searchcontents".into(),
        path: ask.map_or("", |ask| &ask.path).into(),
        msg: contents::message("search.contents.failed", reason),
    };
    let mut line = super::proto::error_line(&error);
    if let Some(ask) = ask {
        line.insert_str(line.len() - 1, &format!(r#", "gen":{}"#, ask.gen));
    }
    super::run::say(out, &line);
}

pub fn done(
    out: &mut impl Write,
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
    tx: &Sender<Event>,
    done: Done,
    watches: (&mut super::watch::Watch, &super::watchpoll::Poller),
) {
    let finished = match st.contents.finish(done, tx) {
        Ok(finished) => finished,
        Err(reason) => {
            error(out, None, &reason);
            return;
        }
    };
    let Some((ask, outcome, close)) = finished else {
        return;
    };
    match outcome {
        Outcome::Ready(snapshot) => {
            super::run::close_search_session(st);
            let _ = super::searchreq::finish_search(out, st, true);
            watches.0.stop();
            watches.1.clear();
            forget_rows(st, pool);
            st.base = PathBuf::from(&ask.path);
            st.held = Held::Walk;
            st.listing = super::listing::Listing::new();
            for hit in &snapshot.matches {
                st.listing.push(&hit.path, false);
            }
            let n = snapshot.matches.len();
            let scanned = snapshot.scanned;
            let listed = super::proto::listed_gen_line(
                n,
                snapshot.ms,
                0.,
                snapshot.dev,
                &ask.path,
                snapshot.writable,
                ask.gen,
            );
            st.contents.held = Some(snapshot);
            metadata(out, st, 0, ask.count);
            super::run::say(out, &super::rowguard::stamped(listed, st.generation));
            rows(out, st, tb, 0, ask.count);
            let ms = st.contents.held.as_ref().map_or(0., |held| held.ms);
            let unsafe_n = st.contents.held.as_ref().map_or(0, |held| {
                held.matches.iter().filter(|hit| !hit.operable).count()
            });
            let partial = st.contents.held.as_ref().is_some_and(|held| held.partial);
            let reason = if partial { "unreadable" } else { "" };
            super::run::say(
                out,
                &format!(
                    r#"{{"t":"searched","n":{n},"scanned":{scanned},"ms":{ms:.3},"cancelled":false,"gen":{},"walking":false,"partial":{partial},"partial_reason":"{reason}","nonoperable":{unsafe_n}}}"#,
                    ask.gen
                ),
            );
        }
        Outcome::Failed(reason) => error(out, Some(&ask), &reason),
        Outcome::Cancelled => cancel_line(out, &ask, close),
    }
}

fn cancel_line(out: &mut impl Write, ask: &Ask, close: bool) {
    let line = if close {
        format!(
            r#"{{"t":"searchclosed","path":"{}","gen":{}}}"#,
            escape(&ask.path),
            ask.gen
        )
    } else {
        format!(
            r#"{{"t":"searched","n":0,"scanned":0,"ms":0.000,"cancelled":true,"gen":{},"walking":false}}"#,
            ask.gen
        )
    };
    super::run::say(out, &line);
}

pub fn cancel(out: &mut impl Write, st: &mut State, close: bool) {
    if close {
        super::run::close_search_session(st);
        let _ = super::searchreq::finish_search(out, st, true);
    }
    if let Some((ask, close)) = st.contents.cancel(close) {
        cancel_line(out, &ask, close);
    }
}

fn metadata(out: &mut impl Write, st: &State, start: usize, count: usize) {
    let Some(held) = &st.contents.held else {
        return;
    };
    let end = start.saturating_add(count).min(held.matches.len());
    let mut from = start.min(end);
    if from == end {
        super::run::say(
            out,
            &ContentsMatches {
                gen: held.ask.gen as u64,
                listing: st.generation,
                start: from,
                matches: Vec::new(),
            }
            .line(),
        );
    }
    while from < end {
        let mut to = from;
        let mut bytes = 0;
        while to < end && (to == from || bytes < METADATA_CHUNK_BYTES) {
            bytes += held.matches[to].line.len() * JSON_TEXT_ESCAPE_BYTES
                + held.matches[to].path_bytes.len() * RAW_PATH_ENVELOPE_BYTES
                + MATCH_ENVELOPE_BYTES;
            to += 1;
        }
        super::run::say(
            out,
            &ContentsMatches {
                gen: held.ask.gen as u64,
                listing: st.generation,
                start: from,
                matches: held.matches[from..to].to_vec(),
            }
            .line(),
        );
        from = to;
    }
}

fn rows(out: &mut impl Write, st: &mut State, tb: &Tables, start: usize, count: usize) {
    let Some(held) = &st.contents.held else {
        return;
    };
    let gen = held.ask.gen;
    let mut rows = super::meta::window_rows(&st.listing, start, count);
    for (offset, row) in rows.iter_mut().enumerate() {
        if !held.matches[start + offset].operable {
            row.cached = Some(super::meta::CachedRow {
                size: 0,
                mtime: 0,
                mode: 0,
                target: String::new(),
                is_symlink: false,
                base_dev: 0,
            });
        }
    }
    let (metas, ms) = super::meta::stat_window_rows(&st.base, &rows, false);
    st.window_meta.clear();
    for (offset, meta) in metas.iter().enumerate() {
        st.window_meta
            .insert(start + offset, (meta.mode, meta.mtime, meta.target_is_dir));
    }
    let line = tb.rows_line(&st.base, &st.listing, &metas, start, ms);
    super::run::say(out, &super::rowguard::stamped_gen(line, st.generation, gen));
}

pub fn window(out: &mut impl Write, st: &mut State, tb: &Tables, start: usize, count: usize) {
    if count > contents::MAX_WINDOW {
        error(out, None, "invalid contents window");
        return;
    }
    let start = start.min(st.listing.len());
    metadata(out, st, start, count);
    rows(out, st, tb, start, count);
}

fn normalized(path: &Path, base: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    };
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            part => out.push(part.as_os_str()),
        }
    }
    out
}

// Every backend read or operation refuses a non-operable row or its display-path alias before resolving it.
pub fn refusal(line: &str, st: &State) -> Option<String> {
    let held = st.contents.held.as_ref()?;
    let command = field_str(line, "c").unwrap_or_default();
    let document = guard::document(line);
    if document.is_err() {
        return Some(identity_error(&command));
    }
    if [
        "searchcontents",
        "searchcancel",
        "searchclose",
        "window",
        "filter",
        "quit",
    ]
    .contains(&command.as_str())
    {
        return None;
    }
    let unsafe_row = |row: usize| held.matches.get(row).is_some_and(|hit| !hit.operable);
    let mut refused = false;
    refused |= field_usize_array(line, "rows").into_iter().any(unsafe_row);
    refused |= field_usize(line, "row").is_some_and(unsafe_row);
    refused |= field_usize(line, "cursor").is_some_and(unsafe_row);
    let document = document.expect("validated contents request");
    fn aliases(value: &Json, st: &State, held: &contents::Snapshot) -> bool {
        match value {
            Json::Str(value) => {
                let path = normalized(Path::new(value), &st.base);
                held.matches.iter().any(|hit| {
                    !hit.operable && normalized(&st.base.join(&hit.path), &st.base) == path
                })
            }
            Json::Arr(values) => values.iter().any(|value| aliases(value, st, held)),
            Json::Obj(values) => values.iter().any(|(_, value)| aliases(value, st, held)),
            _ => false,
        }
    }
    refused |= aliases(&document, st, held);
    refused.then(|| identity_error(&command))
}

fn identity_error(command: &str) -> String {
    super::proto::error_line(&FleaError {
        where_: "contentsidentity".into(),
        path: command.into(),
        msg: contents::message("search.contents.identity", ""),
    })
}
