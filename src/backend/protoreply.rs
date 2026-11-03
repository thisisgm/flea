// Format protocol replies independently of request parsing.
use crate::error::FleaError;
use crate::json::escape;

pub fn located_line(directory: &str, path: &str, index: Option<usize>) -> String {
    let index = index
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-1".into());
    format!(
        r#"{{"t":"located","directory":"{}","path":"{}","index":{}}}"#,
        escape(directory),
        escape(path),
        index
    )
}

pub fn located_many_line(
    directory: &str,
    id: usize,
    transfer_id: usize,
    matches: &[(&str, usize)],
    error: Option<&str>,
) -> String {
    let matches: Vec<_> = matches
        .iter()
        .map(|(path, index)| format!(r#"{{"path":"{}","index":{}}}"#, escape(path), index))
        .collect();
    format!(
        r#"{{"t":"located","directory":"{}","id":{},"transferId":{},"matches":[{}],"ok":{},"error":"{}"}}"#,
        escape(directory),
        id,
        transfer_id,
        matches.join(","),
        error.is_none(),
        escape(error.unwrap_or_default())
    )
}

pub fn listed_line(
    n: usize,
    read_ms: f64,
    sort_ms: f64,
    dev: u64,
    path: &str,
    writable: bool,
) -> String {
    say_listed(n, read_ms, sort_ms, dev, path, writable)
}

// A session answer's listed line, carrying the query gen so a late line lands on no folder.
pub fn listed_gen_line(
    n: usize,
    read_ms: f64,
    sort_ms: f64,
    dev: u64,
    path: &str,
    writable: bool,
    gen: usize,
) -> String {
    let mut line = say_listed(n, read_ms, sort_ms, dev, path, writable);
    if line.ends_with('}') {
        line.insert_str(line.len() - 1, &format!(",\"gen\":{}", gen));
    }
    line
}

// `w` is whether this user can create or delete entries here; a drag from one that cannot copies.
pub(crate) fn say_listed(
    n: usize,
    read_ms: f64,
    sort_ms: f64,
    dev: u64,
    path: &str,
    writable: bool,
) -> String {
    format!(
        r#"{{"t":"listed","n":{},"read":{:.3},"sort":{:.3},"v":{},"w":{},"path":"{}"}}"#,
        n,
        read_ms,
        sort_ms,
        dev,
        writable,
        escape(path)
    )
}

// A listed line with the anchor fields added before its closing brace.
pub(crate) fn with_anchor(listed: &str, anchor: &str, anchor_index: isize) -> String {
    let body = listed.strip_suffix('}').unwrap_or(listed);
    format!(
        r#"{},"anchor":"{}","anchorIndex":{}}}"#,
        body,
        escape(anchor),
        anchor_index
    )
}

// Sample output: {"t":"listed","n":3,"read":0.000,"sort":0.000,"v":1,"w":true,"path":"/d","changed":2}
// A same-path re-list names added plus removed rows, so a rename counts 2 against a net delta of 0.
pub(crate) fn with_changed(listed: &str, changed: usize) -> String {
    let body = listed.strip_suffix('}').unwrap_or(listed);
    format!(r#"{},"changed":{}}}"#, body, changed)
}

// The streaming progress of a search: its own type rather than a listed line, because a mid-walk update is not a fresh listing and carries no read or sort timing.
pub fn searching_line(n: usize, scanned: usize, ms: f64) -> String {
    format!(
        r#"{{"t":"searching","n":{},"scanned":{},"ms":{:.3}}}"#,
        n, scanned, ms
    )
}

// The terminal line of a search: cancelled is true when the client stopped the walk or a new listing replaced it.
pub fn searched_line(n: usize, scanned: usize, ms: f64, cancelled: bool) -> String {
    format!(
        r#"{{"t":"searched","n":{},"scanned":{},"ms":{:.3},"cancelled":{}}}"#,
        n, scanned, ms, cancelled
    )
}

// The file is empty rather than absent on failure, so a client never waits forever for a row that will not arrive.
pub fn thumbed_line(row: usize, file: &str, ms: f64) -> String {
    format!(
        r#"{{"t":"thumbed","row":{},"file":"{}","ms":{:.3}}}"#,
        row,
        escape(file),
        ms
    )
}

// partial is true when the 2000 ms deadline cut the walk short, see docs/protocol.md "dirsized".
pub fn dirsized_line(row: usize, bytes: u64, partial: bool, ms: f64) -> String {
    format!(
        r#"{{"t":"dirsized","row":{},"bytes":{},"partial":{},"ms":{:.3}}}"#,
        row, bytes, partial, ms
    )
}

// Sample output: {"t":"paths","paths":["/home/gm/a.txt","/home/gm/b.txt"]}
pub fn paths_line(paths: &[String]) -> String {
    let mut out = String::from(r#"{"t":"paths","paths":["#);
    for (i, p) in paths.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&escape(p));
        out.push('"');
    }
    out.push_str("]}");
    out
}

// Sample output: {"t":"slow","op":"rename","path":"/hung/a.txt","msg":"/hung is slow. The rename will finish on its own."}
pub fn slow_line(op: &str, path: &str, msg: &str) -> String {
    format!(
        r#"{{"t":"slow","op":"{}","path":"{}","msg":"{}"}}"#,
        escape(op),
        escape(path),
        escape(msg)
    )
}

// Sample output: {"t":"linked","ok":2,"failed":0,"skipped":1}
// Sample output: {"t":"linked","ok":1,"failed":1,"skipped":0,"note":"the link left at /d/b.txt could not be removed (stale); the replaced item stays in the trash"}
pub fn linked_line(ok: usize, failed: usize, skipped: usize, note: &str) -> String {
    if note.is_empty() {
        return format!(
            r#"{{"t":"linked","ok":{},"failed":{},"skipped":{}}}"#,
            ok, failed, skipped
        );
    }
    format!(
        r#"{{"t":"linked","ok":{},"failed":{},"skipped":{},"note":"{}"}}"#,
        ok,
        failed,
        skipped,
        escape(note)
    )
}

// Sample output: {"t":"linktarget","path":"/a/link","directory":"/b","name":"f.txt","id":3}
pub fn linktarget_line(path: &str, directory: &str, name: &str, id: usize) -> String {
    format!(
        r#"{{"t":"linktarget","path":"{}","directory":"{}","name":"{}","id":{}}}"#,
        escape(path),
        escape(directory),
        escape(name),
        id
    )
}

// Sample output: {"t":"permissions","id":7,"op":"applyMany","ok":true,"mode":"0600","error":""}
pub fn permissions_batch_line(id: usize, ok: bool, mode: &str, error: &str) -> String {
    format!(
        r#"{{"t":"permissions","id":{},"op":"applyMany","ok":{},"mode":"{}","error":"{}"}}"#,
        id,
        ok,
        escape(mode),
        escape(error)
    )
}

pub fn error_line(e: &FleaError) -> String {
    format!(
        r#"{{"t":"error","where":"{}","path":"{}","msg":"{}"}}"#,
        escape(&e.where_),
        escape(&e.path),
        escape(&e.msg)
    )
}

// The mode field is written only when the mode is known, so every other error line keeps its three fields.
pub fn error_line_with_mode(e: &FleaError, mode: u32) -> String {
    if mode == 0 {
        return error_line(e);
    }
    format!(
        r#"{{"t":"error","where":"{}","path":"{}","msg":"{}","mode":{}}}"#,
        escape(&e.where_),
        escape(&e.path),
        escape(&e.msg),
        mode
    )
}

// A refusal the UI words itself: the code names the cause, and msg stays for a client that ignores it.
pub fn error_line_with_code(e: &FleaError, code: &str) -> String {
    format!(
        r#"{{"t":"error","where":"{}","path":"{}","msg":"{}","code":"{}"}}"#,
        escape(&e.where_),
        escape(&e.path),
        escape(&e.msg),
        escape(code)
    )
}
