use crate::error::FleaError;
use crate::json::{escape, field_bool, field_str, field_str_array, field_usize, field_usize_array};

// The one spelling of the token the reader thread also matches; see src/backend/events.rs.
pub const TRANSFER_CANCEL: &str = "transfercancel";

pub enum Request {
    List { path: String, first: usize, hidden: bool },
    // A listing built from paths the client names, in the order it named them; the picker's Recent.
    ListPaths { paths: Vec<String>, first: usize },
    Window { start: usize, count: usize },
    // ordering::request reads desc from the raw line, so this parsed copy is read by tests alone; anchor is the cursor's row.
    Sort { by: String, #[cfg_attr(not(test), allow(dead_code))] desc: bool, anchor: Option<String> },
    Search { path: String, query: String, hidden: bool },
    // Unlike thumbcancel there is no rows form: one walk runs at a time, so a cancel can only mean that one.
    SearchCancel,
    // cacheOnly answers from the shared cache alone and never starts a decoder, for a
    // storage class whose switch is off; absent is today's full path, see docs/protocol.md "thumb".
    Thumb { rows: Vec<usize>, cache_only: bool },
    ThumbCancel { rows: Vec<usize> },
    DirSize { rows: Vec<usize> },
    // Unlike thumbcancel, there is no rows form: it always cancels everything in flight, see docs/protocol.md "dirsizecancel".
    DirSizeCancel,
    // The five write operations and their cancel, per the operations design's own wire.
    Transfer { op: String, paths: Vec<String>, rows: Vec<usize>, dest: String, menu_id: usize, shelf: String,
               collide: super::collide::Ask },
    // Which incoming names dest already holds, asked before a transfer; see docs/protocol.md "collisions".
    Collisions { id: usize, paths: Vec<String>, rows: Vec<usize>, dest: String, menu_id: usize },
    TransferCancel { id: usize },
    Trash { paths: Vec<String>, rows: Vec<usize>, menu_id: usize },
    Rename { path: String, to: String, menu_id: usize },
    Duplicate { path: String, menu_id: usize },
    // One new empty directory inside parent path; an empty name asks for the first free "New Folder".
    MkDir { path: String, name: String },
    NewFile { path: String, name: String, id: usize },
    Undo,
    Redo,
    // Resolves row indices to absolute paths, which is what lets a client hold a clipboard for a
    // selection wider than the window it renders; see docs/protocol.md "paths".
    Paths { rows: Vec<usize> },
    Locate { path: String },
    LocateMany { paths: Vec<String>, id: usize, menu_id: usize, transfer_id: usize },
    // The preview column's own extras for one row: pixels, line count, symlink target.
    Meta { row: usize, text: bool, media: bool, archive: bool, token: usize },
    // The status bar's filesystem line for the directory the pane is on.
    FsInfo,
    // A read-only look at a directory that is not the current listing; the columns view's ancestors.
    Peek { path: String, first: usize, hidden: bool, hidden_last: bool, focus: String },
    // op is "compress" or "extract"; a compress names paths and a format, an extract names one path.
    Archive { op: String, paths: Vec<String>, path: String, dest: String, format: String, menu_id: usize },
    Convert { path: String, dest: String, strip: bool, menu_id: usize, request_id: usize, check: bool },
    // Which archive formats this box actually offers, and whether a converter is installed at all.
    Formats { id: usize },
    Permissions { line: String },
    Picker { line: String },
    MenuAction { line: String, rows: Vec<usize> },
    // Directive 71: op is "peers" for the flyout's own list and "send" for the transfer it chooses.
    LocalSend { op: String, peer: String, paths: Vec<String>, id: usize },
    TrashBrowse { line: String },
    // The path bar's folder jump: the favourites and recent files the client read, joined with zoxide's ranking.
    Jump { id: usize, favourites: Vec<String>, recent: Vec<String> },
    // Lightweight: is this path inside a git work tree, and which branch/head.
    GitStatus { id: usize, path: String },
    // Full lane graph for the repo that contains path; limit caps git log --max-count.
    GitGraph { id: usize, path: String, limit: usize },
    Quit,
    Unknown,
}

// Sample input: {"c":"list","path":"/home/gm","first":350,"hidden":false}
pub fn parse_request(line: &str) -> Request {
    match field_str(line, "c").as_deref() {
        Some("trashbrowse") => Request::TrashBrowse { line: line.to_string() },
        Some("permissions") => Request::Permissions { line: line.to_string() },
        Some("picker") => Request::Picker { line: line.to_string() },
        Some("menuaction") => Request::MenuAction { line: line.to_string(), rows: field_usize_array(line, "rows") },
        Some("localsend") => Request::LocalSend {
            op: field_str(line, "op").unwrap_or_default(),
            peer: field_str(line, "peer").unwrap_or_default(),
            paths: field_str_array(line, "paths"),
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("list") => Request::List {
            path: field_str(line, "path").unwrap_or_default(),
            first: field_usize(line, "first").unwrap_or(0),
            // A missing hidden is false, so an older client's request still lists dotfile-free.
            hidden: field_bool(line, "hidden"),
        },
        Some("listpaths") => Request::ListPaths { paths: field_str_array(line, "paths"), first: field_usize(line, "first").unwrap_or(0) },
        Some("window") => Request::Window {
            start: field_usize(line, "start").unwrap_or(0),
            count: field_usize(line, "count").unwrap_or(0),
        },
        Some("sort") => Request::Sort {
            by: field_str(line, "by").unwrap_or_default(),
            desc: field_bool(line, "desc"),
            // An empty anchor is absent: only a cursor row is ever named, never "nothing".
            anchor: field_str(line, "anchor").filter(|anchor| !anchor.is_empty()),
        },
        Some("search") => Request::Search {
            path: field_str(line, "path").unwrap_or_default(),
            query: field_str(line, "query").unwrap_or_default(),
            hidden: field_bool(line, "hidden"),
        },
        Some("searchcancel") => Request::SearchCancel,
        Some("thumb") => Request::Thumb { rows: field_usize_array(line, "rows"),
            cache_only: field_bool(line, "cacheOnly") },
        Some("thumbcancel") => Request::ThumbCancel { rows: field_usize_array(line, "rows") },
        Some("dirsize") => Request::DirSize { rows: field_usize_array(line, "rows") },
        Some("dirsizecancel") => Request::DirSizeCancel,
        Some("transfer") => Request::Transfer {
            // Anything that is not "move" is a copy, so a malformed op can never delete a source.
            op: field_str(line, "op").unwrap_or_default(),
            paths: field_str_array(line, "paths"),
            // The client can only name rows inside the window it holds, so a wide selection is sent as indices instead.
            rows: field_usize_array(line, "rows"),
            dest: field_str(line, "dest").unwrap_or_default(),
            menu_id: field_usize(line, "menuId").unwrap_or(0),
            // A drop out of the shelf carries its single-use token here and names no paths of its own.
            shelf: field_str(line, "shelf").unwrap_or_default(),
            // Absent is today's refusal of an existing name, so an older client's transfer is unchanged.
            collide: super::collide::Ask::parse(line),
        },
        Some("collisions") => Request::Collisions {
            id: field_usize(line, "id").unwrap_or(0),
            paths: field_str_array(line, "paths"),
            rows: field_usize_array(line, "rows"),
            dest: field_str(line, "dest").unwrap_or_default(),
            menu_id: field_usize(line, "menuId").unwrap_or(0),
        },
        Some(TRANSFER_CANCEL) => Request::TransferCancel { id: field_usize(line, "id").unwrap_or(0) },
        Some("trash") => Request::Trash {
            paths: field_str_array(line, "paths"),
            rows: field_usize_array(line, "rows"),
            menu_id: field_usize(line, "menuId").unwrap_or(0),
        },
        Some("rename") => Request::Rename {
            path: field_str(line, "path").unwrap_or_default(),
            to: field_str(line, "to").unwrap_or_default(),
            menu_id: field_usize(line, "menuId").unwrap_or(0),
        },
        Some("duplicate") => Request::Duplicate { path: field_str(line, "path").unwrap_or_default(), menu_id: field_usize(line, "menuId").unwrap_or(0) },
        Some("mkdir") => Request::MkDir { path: field_str(line, "path").unwrap_or_default(), name: field_str(line, "name").unwrap_or_default() },
        Some("newfile") => Request::NewFile {
            path: field_str(line, "path").unwrap_or_default(),
            name: field_str(line, "name").unwrap_or_default(),
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("undo") => Request::Undo,
        Some("redo") => Request::Redo,
        Some("paths") => Request::Paths { rows: field_usize_array(line, "rows") },
        Some("locate") => match field_str(line, "path") {
            Some(path) => Request::Locate { path },
            None => Request::LocateMany { paths: field_str_array(line, "paths"),
                id: field_usize(line, "id").unwrap_or(0), menu_id: field_usize(line, "menuId").unwrap_or(0),
                transfer_id: field_usize(line, "transferId").unwrap_or(0) },
        },
        Some("fsinfo") => Request::FsInfo,
        Some("archive") => Request::Archive {
            menu_id: field_usize(line, "menuId").unwrap_or(0),
            // Anything that is not "compress" is an extract, so a malformed op never writes an archive.
            op: field_str(line, "op").unwrap_or_default(),
            paths: field_str_array(line, "paths"),
            path: field_str(line, "path").unwrap_or_default(),
            dest: field_str(line, "dest").unwrap_or_default(),
            format: field_str(line, "format").unwrap_or_default(),
        },
        Some("convert") => Request::Convert {
            request_id: field_usize(line, "requestId").unwrap_or(0),
            check: field_bool(line, "check"),
            menu_id: field_usize(line, "menuId").unwrap_or(0),
            path: field_str(line, "path").unwrap_or_default(),
            dest: field_str(line, "dest").unwrap_or_default(),
            strip: field_bool(line, "strip"),
        },
        Some("formats") => Request::Formats { id: field_usize(line, "id").unwrap_or(0) },
        Some("peek") => Request::Peek {
            path: field_str(line, "path").unwrap_or_default(),
            first: field_usize(line, "first").unwrap_or(0),
            hidden: field_bool(line, "hidden"),
            // Absent is today's order, so an older client's peek still sorts dotfiles first.
            hidden_last: field_bool(line, "hiddenLast"),
            focus: field_str(line, "focus").unwrap_or_default(),
        },
        Some("meta") => Request::Meta {
            token: field_usize(line, "token").unwrap_or(0),
            row: field_usize(line, "row").unwrap_or(0),
            text: field_bool(line, "text"),
            media: field_bool(line, "media"),
            archive: field_bool(line, "archive"),
        },
        Some("jump") => Request::Jump { id: field_usize(line, "id").unwrap_or(0), favourites: field_str_array(line, "favourites"), recent: field_str_array(line, "recent") },

        Some("gitstatus") => Request::GitStatus {
            id: field_usize(line, "id").unwrap_or(0),
            path: field_str(line, "path").unwrap_or_default(),
        },
        Some("gitgraph") => Request::GitGraph {
            id: field_usize(line, "id").unwrap_or(0),
            path: field_str(line, "path").unwrap_or_default(),
            limit: field_usize(line, "limit").unwrap_or(0),
        },        Some("quit") => Request::Quit,
        _ => Request::Unknown,
    }
}

pub fn located_line(directory: &str, path: &str, index: Option<usize>) -> String {
    let index = index.map(|value| value.to_string()).unwrap_or_else(|| "-1".into());
    format!(r#"{{"t":"located","directory":"{}","path":"{}","index":{}}}"#,
        escape(directory), escape(path), index)
}

pub fn located_many_line(directory: &str, id: usize, transfer_id: usize, matches: &[(&str, usize)], error: Option<&str>) -> String {
    let matches: Vec<_> = matches.iter().map(|(path, index)|
        format!(r#"{{"path":"{}","index":{}}}"#, escape(path), index)).collect();
    format!(r#"{{"t":"located","directory":"{}","id":{},"transferId":{},"matches":[{}],"ok":{},"error":"{}"}}"#,
        escape(directory), id, transfer_id, matches.join(","), error.is_none(), escape(error.unwrap_or_default()))
}

pub fn listed_line(n: usize, read_ms: f64, sort_ms: f64, dev: u64, path: &str) -> String {
    format!(
        r#"{{"t":"listed","n":{},"read":{:.3},"sort":{:.3},"v":{},"path":"{}"}}"#,
        n, read_ms, sort_ms, dev, escape(path)
    )
}

// The anchor's index in the new order, or -1; the fields ride last, so an unanchored reply is the line above exactly.
pub fn listed_line_anchor(n: usize, read_ms: f64, sort_ms: f64, dev: u64, path: &str, anchor: &str, anchor_index: isize) -> String {
    format!(
        r#"{{"t":"listed","n":{},"read":{:.3},"sort":{:.3},"v":{},"path":"{}","anchor":"{}","anchorIndex":{}}}"#,
        n, read_ms, sort_ms, dev, escape(path), escape(anchor), anchor_index
    )
}

// The streaming progress of a search: its own type rather than a listed line, because a mid-walk update is not a fresh listing and carries no read or sort timing.
pub fn searching_line(n: usize, scanned: usize, ms: f64) -> String {
    format!(r#"{{"t":"searching","n":{},"scanned":{},"ms":{:.3}}}"#, n, scanned, ms)
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
    format!(r#"{{"t":"thumbed","row":{},"file":"{}","ms":{:.3}}}"#, row, escape(file), ms)
}

// partial is true when the 2000 ms deadline cut the walk short, see docs/protocol.md "dirsized".
pub fn dirsized_line(row: usize, bytes: u64, partial: bool, ms: f64) -> String {
    format!(r#"{{"t":"dirsized","row":{},"bytes":{},"partial":{},"ms":{:.3}}}"#, row, bytes, partial, ms)
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

pub fn error_line(e: &FleaError) -> String {
    format!(
        r#"{{"t":"error","where":"{}","path":"{}","msg":"{}"}}"#,
        escape(&e.where_),
        escape(&e.path),
        escape(&e.msg)
    )
}

// A denied listing is the only failure a pane draws more than a sentence for: States.dc.html gives
// it the directory's own mode string. The field is written only when the mode is known, so every
// other error line on this wire keeps exactly the three fields it has always had.
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

#[cfg(test)]
#[path = "proto_tests.rs"]
mod tests;
