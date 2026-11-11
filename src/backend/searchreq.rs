// The wire side of a search: the loop hands it a slice of walking and it decides what to say, the way thumbreq answers thumb.
use crate::backend::proto::{searched_line, searching_line};
use crate::backend::state::{Held, State};
use crate::backend::timing::since;
use crate::backend::undo::Journal;
use flea::bytename::ExactText as _;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// A streaming search announces its growing count no more often than this, so a fast walk cannot flood the client's parser.
const SEARCH_REPORT: Duration = Duration::from_millis(100);

// One bounded slice per call, so an opcancel or a keystroke is never behind a whole subtree. Answers
// whether the walk ended in a new row order, which is what the caller forgets its row indices on.
pub fn step_search(out: &mut impl Write, st: &mut State) -> bool {
    let done = match st.search.as_mut() {
        Some(s) => s.step(&mut st.listing),
        None => return false,
    };
    if done {
        return finish_search(out, st, false);
    }
    if st.search_reported.elapsed() >= SEARCH_REPORT {
        st.search_reported = Instant::now();
        let (scanned, ms) = match st.search.as_ref() {
            Some(s) => (s.scanned, since(s.started)),
            None => return false,
        };
        writeln!(out, "{}", searching_line(st.listing.len(), scanned, ms)).ok();
        out.flush().ok();
    }
    false
}

// searched carries the final count after the rows are ranked, so no listed line trails it and every held row index is stale; see docs/protocol.md "searched".
pub fn finish_search(out: &mut impl Write, st: &mut State, cancelled: bool) -> bool {
    let s = match st.search.take() {
        Some(s) => s,
        None => return false,
    };
    let reordered = s.rank(&mut st.listing);
    let ms = since(s.started);
    writeln!(out, "{}", searched_line(st.listing.len(), s.scanned, ms, cancelled)).ok();
    out.flush().ok();
    reordered
}

// A session answer's own open line: the walk starts at once and no rows change.
pub fn searchopened_line(path: &str) -> String {
    format!(r#"{{"t":"searchopened","path":"{}"}}"#, crate::json::escape(path))
}

// A session answer's own close line, after the index is dropped.
pub fn searchclosed_line(path: &str) -> String {
    format!(r#"{{"t":"searchclosed","path":"{}"}}"#, crate::json::escape(path))
}

// Sample output: {"t":"searched","n":11,"scanned":1040,"ms":12.400,"cancelled":false,"gen":7,"walking":false}
#[cfg(test)]
pub fn searched_gen_line(n: usize, scanned: usize, ms: f64, gen: usize, walking: bool) -> String {
    searched_status_line(n, scanned, ms, gen, walking, None)
}

pub(crate) fn searched_status_line(n: usize, scanned: usize, ms: f64, gen: usize, walking: bool, reason: Option<&str>) -> String {
    let reason = reason.unwrap_or(if walking { "walking" } else { "" });
    format!(
        r#"{{"t":"searched","n":{},"scanned":{},"ms":{:.3},"cancelled":false,"gen":{},"walking":{},"partial":{},"partial_reason":"{}"}}"#,
        n, scanned, ms, gen, walking, !reason.is_empty(), reason
    )
}

// One answered query: the window head's rows in rank order, plus its populations.
pub(crate) struct Answer {
    pub gen: usize,
    // Every match, whatever the window holds, less hidden typo rows when typos is false: narrowing scores this set, not the head.
    pub all: Vec<usize>,
    // True when all holds every typo match too; a strict answer leaves out the typo rows it could not show.
    pub typos: bool,
    pub rows: Vec<(String, bool)>,
    pub total: usize,
    pub indexed: usize,
    pub walking: bool,
    pub partial: Option<&'static str>,
    pub ms: f64,
    pub count: usize,
    pub scope: PathBuf,
    pub dev: u64,
    pub writable: bool,
}

// A stopped walk's re-answer: the standing query's rows or none, with the generation and frozen index size it keeps.
pub(crate) struct Cancelled {
    pub answer: Option<Answer>,
    pub gen: usize,
    pub indexed: usize,
}

// True for a line naming a session query, so the loop collapses a burst before answering.
pub fn is_search_query_line(line: &str) -> bool {
    crate::json::field_str(line, "c").as_deref() == Some("searchquery")
}

// One query line as its parts: the text, the generation and the wanted first window.
pub fn parse_search_query(line: &str) -> (String, usize, Option<usize>) {
    (
        crate::json::field_str(line, "query").unwrap_or_default(),
        crate::json::field_usize(line, "gen").unwrap_or(0),
        crate::json::field_usize(line, "count"),
    )
}

// The collapsed newest query back on the wire, so it answers through the one query arm.
#[cfg(test)]
pub fn render_search_query(query: &(String, usize, Option<usize>)) -> String {
    let count = query.2.map(|count| format!(r#","count":{}"#, count)).unwrap_or_default();
    format!(r#"{{"c":"searchquery","query":"{}","gen":{}{}}}"#, crate::json::escape(&query.0), query.1, count)
}

// Drains a burst in arrival order, returning its highest generation even when its newest line waits behind a sibling.
pub fn collapse_query_burst(
    rx: &std::sync::mpsc::Receiver<crate::backend::events::Event>,
    line: String,
    buffered: &mut std::collections::VecDeque<crate::backend::events::Event>,
) -> (Option<String>, usize) {
    let mut newest = parse_search_query(&line);
    let mut newest_line = line.clone();
    let mut rest = Vec::new();
    // How many drained siblings arrived before the newest query, so it answers in its own place.
    let mut at = 0;
    while let Ok(event) = rx.try_recv() {
        match event {
            crate::backend::events::Event::Request(next) if is_search_query_line(&next) => {
                let query = parse_search_query(&next);
                if query.1 >= newest.1 {
                    newest = query;
                    newest_line = next;
                    at = rest.len();
                }
            }
            other => rest.push(other),
        }
    }
    let after = rest.split_off(at);
    let now = if at == 0 && newest_line == line {
        Some(line)
    } else {
        buffered.extend(rest);
        buffered.push_back(crate::backend::events::Event::Request(newest_line));
        None
    };
    buffered.extend(after);
    (now, newest.1)
}

// Materialises one answered query as the listing: listed with the count, the first window, then searched with the gen.
pub fn emit_session_answer(
    out: &mut impl Write,
    st: &mut State,
    tb: &crate::backend::state::Tables,
    pool: &crate::backend::thumbs::Pool,
    answer: &Answer,
) {
    emit_session_answer_cancelled(out, st, tb, pool, answer, false);
}

// The cancelled re-answer: the same three lines, with the terminal line naming the stop.
pub fn emit_session_answer_cancelled(
    out: &mut impl Write,
    st: &mut State,
    tb: &crate::backend::state::Tables,
    pool: &crate::backend::thumbs::Pool,
    answer: &Answer,
    cancelled: bool,
) {
    super::searchremap::landing(st, answer.gen, &answer.scope);
    st.base = answer.scope.clone();
    let names = answer.rows.iter().map(|(path, _)| path.len()).sum();
    st.listing = crate::backend::listing::Listing::with_reserve(names, answer.rows.len());
    for (path, is_dir) in &answer.rows {
        st.listing.push(path, *is_dir);
    }
    // A session answer is a result list and not a directory, the walk's own Held.
    st.held = Held::Walk;
    super::run::forget_rows(st, pool);
    super::answersizes::fill(st, 0, answer.count);
    writeln!(
        out,
        "{}",
        super::proto::listed_gen_line(
            answer.total,
            0.0,
            answer.ms,
            answer.dev,
            &answer.scope.exact_text(),
            answer.writable,
            answer.gen
        )
    )
    .ok();
    super::run::write_window_gen(out, st, 0, answer.count, tb, Some(answer.gen));
    if cancelled {
        writeln!(out, "{}", searched_cancelled_line(answer.total, answer.indexed, answer.ms, answer.gen)).ok();
    } else {
        writeln!(out, "{}", searched_status_line(answer.total, answer.indexed, answer.ms, answer.gen, answer.walking, answer.partial)).ok();
    }
    out.flush().ok();
}

// The terminal line of a stopped walk: the gen it keeps, walking false, cancelled true.
pub fn searched_cancelled_line(n: usize, scanned: usize, ms: f64, gen: usize) -> String {
    format!(
        r#"{{"t":"searched","n":{},"scanned":{},"ms":{:.3},"cancelled":true,"gen":{},"walking":false,"partial":true,"partial_reason":"cancelled"}}"#,
        n, scanned, ms, gen
    )
}

// The paths one landed write touched, so the session patches exactly their directories.
pub fn op_write_paths(msg: &crate::backend::opsreq::OpMsg) -> Vec<PathBuf> {
    use crate::backend::opsreq::OpMsg;
    let mut paths = Vec::new();
    match msg {
        OpMsg::CreationReceipt { inner, .. } => return op_write_paths(inner),
        OpMsg::TransferDone { entry, .. }
        | OpMsg::MergeDone { entry, .. }
        | OpMsg::Trashed { entry, .. }
        | OpMsg::Linked { entry, .. }
        | OpMsg::Pasted { entry: Some(entry), .. } => paths.extend(Journal::touched_paths(&entry.steps)),
        // The landed steps count whether or not the batch finished.
        OpMsg::BulkRenameDone { result: Ok((steps, _)), .. } => paths.extend(Journal::touched_paths(steps)),
        OpMsg::Duplicated { path, .. } => paths.push(flea::bytename::exact(path)),
        OpMsg::RenameDone { result, .. } | OpMsg::MkdirDone { result, .. } => {
            if let Ok((path, steps)) = result {
                paths.push(path.clone());
                paths.extend(Journal::touched_paths(steps));
            }
        }
        OpMsg::RedoDone { touched, .. } => paths.extend(touched.dirs.clone()),
        _ => {}
    }
    paths
}

// The files whose tags one landed write changed, so the session re-reads exactly their names and patches no directory.
pub fn op_tag_paths(msg: &crate::backend::opsreq::OpMsg) -> Vec<PathBuf> {
    use crate::backend::opsreq::OpMsg;
    match msg {
        OpMsg::SetTagsDone { result: Ok((_, steps, _, _)), .. } => crate::backend::undo::Reached::of(steps).tags,
        OpMsg::RedoDone { touched, .. } => touched.tags.clone(),
        _ => Vec::new(),
    }
}





#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_lines_carry_their_generation() {
        let searched = searched_gen_line(11, 1040, 12.4, 7, false);
        assert_eq!(crate::json::field_str(&searched, "t").as_deref(), Some("searched"));
        assert_eq!(crate::json::field_usize(&searched, "gen"), Some(7));
        assert_eq!(crate::json::field_usize(&searched, "n"), Some(11));
        let opened = searchopened_line("/scope");
        assert_eq!(crate::json::field_str(&opened, "t").as_deref(), Some("searchopened"));
        let closed = searchclosed_line("/scope");
        assert_eq!(crate::json::field_str(&closed, "t").as_deref(), Some("searchclosed"));
    }

    #[test]
    fn a_query_line_parses_and_renders_back() {
        let line = r#"{"c":"searchquery","query":"bench","gen":7,"count":100}"#;
        assert!(is_search_query_line(line));
        assert!(!is_search_query_line(r#"{"c":"search","query":"bench"}"#));
        let parsed = parse_search_query(line);
        assert_eq!(parsed, ("bench".to_string(), 7, Some(100)));
        let rendered = render_search_query(&parsed);
        assert_eq!(parse_search_query(&rendered), parsed, "a collapsed query re-parses");
        assert_eq!(parse_search_query(r#"{"c":"searchquery","query":"b"}"#).1, 0, "a missing gen never answers");
    }

    // The request lines a drained burst left in `buffered`, in the order the loop runs them.
    fn queued(buffered: &std::collections::VecDeque<crate::backend::events::Event>) -> Vec<String> {
        buffered.iter().map(|event| match event {
            crate::backend::events::Event::Request(line) => line.clone(),
            _ => String::from("other"),
        }).collect()
    }

    #[test]
    fn a_burst_of_five_queries_keeps_only_the_last() {
        let (tx, rx) = std::sync::mpsc::channel();
        for gen in 1..=5 {
            tx.send(crate::backend::events::Event::Request(format!(
                r#"{{"c":"searchquery","query":"bench{}","gen":{}}}"#,
                gen, gen
            )))
            .expect("the burst queues");
        }
        let mut buffered = std::collections::VecDeque::new();
        let now = collapse_query_burst(&rx, r#"{"c":"searchquery","query":"bench0","gen":0}"#.to_string(), &mut buffered).0;
        assert!(now.is_none(), "the first query is superseded");
        assert_eq!(queued(&buffered), [r#"{"c":"searchquery","query":"bench5","gen":5}"#], "only the last of the 5 answers");
    }

    #[test]
    fn a_collapsed_query_answers_in_its_own_place_among_its_siblings() {
        let (tx, rx) = std::sync::mpsc::channel();
        for line in [r#"{"c":"thumbcancel","rows":[]}"#, r#"{"c":"searchquery","query":"b2","gen":2}"#, r#"{"c":"searchclose"}"#] {
            tx.send(crate::backend::events::Event::Request(line.to_string())).expect("the burst queues");
        }
        let mut buffered = std::collections::VecDeque::new();
        let now = collapse_query_burst(&rx, r#"{"c":"searchquery","query":"b1","gen":1}"#.to_string(), &mut buffered).0;
        assert!(now.is_none(), "the newer generation wins");
        assert_eq!(queued(&buffered), [r#"{"c":"thumbcancel","rows":[]}"#, r#"{"c":"searchquery","query":"b2","gen":2}"#, r#"{"c":"searchclose"}"#],
            "the sibling sent before b2 runs before it, the close sent after runs after it");
    }

    #[test]
    fn a_newest_first_query_answers_before_the_siblings_drained_after_it() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(crate::backend::events::Event::Request(r#"{"c":"searchopen","path":"/other"}"#.to_string())).expect("the open queues");
        let mut buffered = std::collections::VecDeque::new();
        let first = r#"{"c":"searchquery","query":"b5","gen":5}"#.to_string();
        let now = collapse_query_burst(&rx, first.clone(), &mut buffered).0;
        assert_eq!(now, Some(first), "gen 5 answers now, against the scope it was sent for");
        assert_eq!(queued(&buffered), [r#"{"c":"searchopen","path":"/other"}"#], "the open sent after it waits");
    }

    #[test]
    fn landed_writes_map_to_their_touched_paths() {
        use crate::backend::opsreq::OpMsg;
        use crate::backend::undo::{Entry, Step};
        fn identity() -> crate::backend::undo::ItemIdentity {
            crate::backend::undo::ItemIdentity::from_parts(1, 2, 0o100000, 3, (4, 5), (6, 7), None)
        }
        let moved = OpMsg::TransferDone {
            id: 1,
            ok: 1,
            failed: 0,
            skipped: 0,
            cancelled: false,
            entry: Entry {
                op: "move".to_string(),
                steps: vec![Step::Moved {
                    from: PathBuf::from("/scope/a.txt"),
                    to: PathBuf::from("/scope/sub/b.txt"),
                    before: identity(),
                    after: identity(),
                    rename_fat: false,
                }],
            },
            retry: Vec::new(),
            durable: false,
            note: String::new(),
            refused: Vec::new(),
        };
        let dirs = op_write_paths(&moved);
        assert_eq!(dirs, vec![PathBuf::from("/scope/a.txt"), PathBuf::from("/scope/sub/b.txt")], "both touched paths patch");
        let progress: OpMsg = OpMsg::SlotDone { line: String::new() };
        assert!(op_write_paths(&progress).is_empty(), "a terminal line with no paths patches nothing");
        let redone: OpMsg = OpMsg::RedoDone {
            journal: Box::new(crate::backend::undo::Journal::new()),
            result: Ok("rename".to_string()),
            touched: crate::backend::undo::Reached { dirs: vec![PathBuf::from("/scope/back.txt")], tags: Vec::new() },
        };
        assert_eq!(op_write_paths(&redone), vec![PathBuf::from("/scope/back.txt")], "a redo patches its snapshot");
    }

    #[test]
    fn a_merge_a_paste_and_a_bulk_rename_patch_the_session_like_a_transfer() {
        use crate::backend::opsreq::OpMsg;
        use crate::backend::undo::{Entry, Step};
        let identity = || crate::backend::undo::ItemIdentity::from_parts(1, 2, 0o100000, 3, (4, 5), (6, 7), None);
        let moved = || Step::Moved { from: PathBuf::from("/scope/a.txt"), to: PathBuf::from("/scope/sub/b.txt"), before: identity(), after: identity(), rename_fat: false };
        let both = vec![PathBuf::from("/scope/a.txt"), PathBuf::from("/scope/sub/b.txt")];
        let merged = OpMsg::MergeDone {
            id: 1, ok: 1, failed: 0, skipped: 0, cancelled: false,
            entry: Entry { op: "merge".to_string(), steps: vec![moved()] },
            retry: Vec::new(), durable: false, note: String::new(), merged: 1, kept_both: 0, into: "sub".to_string(), refused: Vec::new(),
        };
        assert_eq!(op_write_paths(&merged), both, "a merge patches the folders it moved and filled");
        let pasted = OpMsg::Pasted { line: String::new(), entry: Some(Entry { op: "paste".to_string(), steps: vec![moved()] }) };
        assert_eq!(op_write_paths(&pasted), both, "a pasted image patches the folder it landed in");
        assert!(op_write_paths(&OpMsg::Pasted { line: String::new(), entry: None }).is_empty(), "a refused paste patches nothing");
        let renamed = OpMsg::BulkRenameDone { id: 2, result: Ok((vec![moved()], Ok((1, 0)))) };
        assert_eq!(op_write_paths(&renamed), both, "a late bulk rename patches what it renamed");
        let partial = OpMsg::BulkRenameDone { id: 3, result: Ok((vec![moved()], Err(crate::error::FleaError { where_: "bulkrename".into(), path: String::new(), msg: "x".into() }))) };
        assert_eq!(op_write_paths(&partial), both, "a batch that failed part way patches what landed");
    }
}
