use crate::error::FleaError;
use crate::json::{escape, field_bool, field_str, field_str_array, field_usize, field_usize_array};

// The one spelling of the token the reader thread also matches; see src/backend/events.rs.
pub const TRANSFER_CANCEL: &str = "transfercancel";

pub enum Request {
    List { path: String, first: usize, hidden: bool, want_changed: bool },
    // A listing built from paths the client names, in the order it named them; the picker's Recent.
    ListPaths { paths: Vec<String>, first: usize },
    Window { start: usize, count: usize },
    // ordering::request reads desc from the raw line, so this parsed copy is read by tests alone; anchor is the cursor's row.
    Sort { by: String, #[cfg_attr(not(test), allow(dead_code))] desc: bool, anchor: Option<String> },
    Search { path: String, query: String, hidden: bool },
    // Unlike thumbcancel there is no rows form: one walk runs at a time, so a cancel can only mean that one.
    SearchCancel,
    // A live session: the scope walks once, each query answers from the index with its own generation.
    SearchOpen { path: String, hidden: bool, folder: String },
    SearchQuery { query: String, gen: usize, count: Option<usize>, sizes: Option<usize> },
    SearchClose,
    // The filter's three, issue 315: a query over the listing's names with its first view window, a later view window, and the close.
    Filter { query: String, gen: usize, at: usize, before: usize, count: usize },
    FilterWindow { start: usize, count: usize },
    FilterClose,
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
    // Which incoming names dest already holds, asked before a transfer, or after Merge for the folders it merges; see docs/protocol.md "collisions".
    Collisions { id: usize, paths: Vec<String>, rows: Vec<usize>, dest: String, menu_id: usize, merge: bool, fat: Option<super::fatnames::Choice> },
    // A clipboard image as a named file; see docs/protocol.md "pasteimage".
    PasteImage(super::pasteimage::Ask),
    TransferCancel { id: usize },
    Trash { paths: Vec<String>, rows: Vec<usize>, menu_id: usize },
    Rename { path: String, to: String, menu_id: usize },
    // Bulk rename: parallel paths and bare new names, validated whole before any rename.
    BulkRename { paths: Vec<String>, names: Vec<String> },
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
    // The cursor row's two-byte shebang probe for the Make executable row; see docs/protocol.md "shebang".
    Shebang { path: String, id: usize },
    // The code preview's coloured lines for a file or a text the client holds; the wire lives in codereq.rs.
    Code(super::codereq::Ask),
    Html(super::htmlreq::Ask),
    // Two regular files answered with their unified diff, or the reason there is none; the wire lives in compare.rs.
    Compare(super::compare::Ask),
    // One tag change across rows of the listing in force: add, remove, toggle or clear.
    SetTags { rows: Vec<usize>, op: String, tag: String },
    // Which of Flea's seven tags those rows all carry or some carry, read only; see docs/protocol.md "tagstates".
    TagStates { rows: Vec<usize>, id: usize },
    // The status bar's filesystem line for the directory the pane is on.
    FsInfo,
    // A read-only look at a directory that is not the current listing; the columns view's ancestors.
    Peek { path: String, first: usize, hidden: bool, hidden_last: bool, focus: String, watch: bool, keep: Vec<String>, by: Option<String>, desc: bool },
    // A PDF fetched into a session-private copy under a deadline, so a hung mount never freezes the window.
    PdfCopy { id: usize, slot: String, path: String },
    // An office document converted once to a cached PDF by LibreOffice; answered ready, converting or failed.
    OfficePdf { id: usize, path: String },
    // Quick Look's font page: the facts and three sample images for one font; see src/backend/fontinfo/render.rs.
    FontLook(super::fontinfo::render::Ask),
    // op is "compress" or "extract"; a compress names paths and a format, an extract names one path.
    // password travels this stdin wire and leaves it only on the tool's own stdin pipe, never in argv.
    Archive { op: String, paths: Vec<String>, path: String, dest: String, format: String, menu_id: usize,
              password: String, encrypt_names: bool },
    Convert { path: String, dest: String, strip: bool, menu_id: usize, request_id: usize, check: bool },
    // Which archive formats this box actually offers, and whether a converter is installed at all.
    Formats { id: usize },
    Permissions { line: String },
    // Paste as links: one symlink or hard link per source inside dest; see docs/protocol.md "link".
    Link { op: String, paths: Vec<String>, rows: Vec<usize>, dest: String,
           collide: super::collide::Ask },
    // Show original: where a symlink's target lives, the same path Show in
    // folder uses; see docs/protocol.md "linktarget".
    LinkTarget { path: String, id: usize },
    // Permissions for the whole selection: one Entry holds every path the
    // Apply changed, so one undo restores them all; see docs/protocol.md
    // "permissionsBatch".
    PermissionsBatch { paths: Vec<String>, modes: Vec<String>, id: usize },
    Picker { line: String },
    MenuAction { line: String, rows: Vec<usize> },
    // Directive 71: op is "peers" for the flyout's own list and "send" for the transfer it chooses.
    LocalSend { op: String, peer: String, paths: Vec<String>, id: usize },
    TrashBrowse { line: String },
    // The device of each path named, symlinks followed, for the drop verb of a target that is not the listing; see docs/protocol.md "devs".
    Devs { paths: Vec<String>, id: usize },
    // The path bar's folder jump: the favourites and recent files the client read, joined with zoxide's ranking.
    Jump { id: usize, ranking: usize, favourites: Vec<String>, recent: Vec<String> },
    // The system clipboard for files, owned across windows; see docs/protocol.md "clip".
    ClipSet { op: String, paths: Vec<String> },
    ClipGet,
    ClipClear { token: String, cut: Vec<String> },
    ClipWatch,
    // The image flasher's backend; the five requests live in flash::Req beside their wire.
    Flash(super::flash::Req),
    Quit,
    Unknown,
}

// Sample input: {"c":"list","path":"/home/gm","first":350,"hidden":false}
pub fn parse_request(line: &str) -> Request {
    match field_str(line, "c").as_deref() {
        Some("trashbrowse") => Request::TrashBrowse { line: line.to_string() },
        Some("permissions") => Request::Permissions { line: line.to_string() },
        Some("permissionsBatch") => Request::PermissionsBatch {
            paths: field_str_array(line, "paths"),
            modes: field_str_array(line, "modes"),
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("link") => Request::Link {
            // Anything that is not "absolute" or "hard" is a relative link, never an absolute path by accident.
            op: field_str(line, "op").unwrap_or_default(),
            paths: field_str_array(line, "paths"),
            rows: field_usize_array(line, "rows"),
            dest: field_str(line, "dest").unwrap_or_default(),
            collide: super::collide::Ask::parse(line),
        },
        Some("linktarget") => Request::LinkTarget { path: field_str(line, "path").unwrap_or_default(), id: field_usize(line, "id").unwrap_or(0) },
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
            // Absent is today's silent re-read, so an older client never pays the count.
            want_changed: field_bool(line, "wantChanged"),
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
        Some("searchopen") => Request::SearchOpen {
            path: field_str(line, "path").unwrap_or_default(),
            hidden: field_bool(line, "hidden"),
            folder: field_str(line, "folder").unwrap_or_default(),
        },
        Some("searchquery") => Request::SearchQuery {
            query: field_str(line, "query").unwrap_or_default(),
            // Absent is never newer, so a query without a generation answers nothing.
            gen: field_usize(line, "gen").unwrap_or(0),
            // Absent is the list's own first-window default, read where the answer is built.
            count: field_usize(line, "count"),
            sizes: field_usize(line, "sizes"),
        },
        Some("searchclose") => Request::SearchClose,
        Some("filter") => Request::Filter {
            query: field_str(line, "query").unwrap_or_default(),
            // Absent is never newer, so a query without a generation answers nothing.
            gen: field_usize(line, "gen").unwrap_or(0),
            at: field_usize(line, "at").unwrap_or(0),
            before: field_usize(line, "before").unwrap_or(0),
            count: field_usize(line, "count").unwrap_or(0),
        },
        Some("filterwindow") => Request::FilterWindow {
            start: field_usize(line, "start").unwrap_or(0),
            count: field_usize(line, "count").unwrap_or(0),
        },
        Some("filterclose") => Request::FilterClose,
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
        Some(word @ ("collisions" | "mergecollisions")) => Request::Collisions {
            id: field_usize(line, "id").unwrap_or(0),
            paths: field_str_array(line, "paths"),
            rows: field_usize_array(line, "rows"),
            dest: field_str(line, "dest").unwrap_or_default(),
            menu_id: field_usize(line, "menuId").unwrap_or(0),
            merge: word == "mergecollisions",
            fat: super::fatnames::Choice::parse(line),
        },
        Some("pasteimage") => Request::PasteImage(super::pasteimage::Ask::parse(line)),
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
        // Sample input: {"c":"bulkrename","paths":["/d/a.txt","/d/b.txt"],"names":["b.txt","a.txt"]}
        Some("bulkrename") => Request::BulkRename {
            paths: field_str_array(line, "paths"),
            names: field_str_array(line, "names"),
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
            // Absent is no password, so every older client keeps today's path.
            password: field_str(line, "password").unwrap_or_default(),
            encrypt_names: field_bool(line, "encryptNames"),
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
        Some("fontlook") => Request::FontLook(super::fontinfo::render::Ask::parse(line)),
        Some("pdfcopy") => Request::PdfCopy {
            id: field_usize(line, "id").unwrap_or(0),
            // Absent is one viewer, so an older client's fetch keeps the shared slot.
            slot: field_str(line, "slot").unwrap_or_default(),
            path: field_str(line, "path").unwrap_or_default(),
        },
        Some("officepdf") => Request::OfficePdf {
            id: field_usize(line, "id").unwrap_or(0),
            path: field_str(line, "path").unwrap_or_default(),
        },
        Some("peek") => Request::Peek {
            path: field_str(line, "path").unwrap_or_default(),
            first: field_usize(line, "first").unwrap_or(0),
            hidden: field_bool(line, "hidden"),
            // Absent is today's order, so an older client's peek still sorts dotfiles first.
            hidden_last: field_bool(line, "hiddenLast"),
            focus: field_str(line, "focus").unwrap_or_default(),
            // Only the columns view asks to be told when this directory changes.
            watch: field_bool(line, "watch"),
            // Sample input: "keep":["/home","/home/gm"], every column directory the view draws; the rest stop being watched.
            keep: field_str_array(line, "keep"),
            // Absent is name order, so an older client's peek reads as it did.
            by: field_str(line, "by"),
            desc: field_bool(line, "desc"),
        },
        Some("meta") => Request::Meta {
            token: field_usize(line, "token").unwrap_or(0),
            row: field_usize(line, "row").unwrap_or(0),
            text: field_bool(line, "text"),
            media: field_bool(line, "media"),
            archive: field_bool(line, "archive"),
        },
        Some("shebang") => Request::Shebang {
            path: field_str(line, "path").unwrap_or_default(),
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("code") => Request::Code(super::codereq::Ask::parse(line)),
        Some("html") => Request::Html(super::htmlreq::Ask::parse(line)),
        Some("compare") => Request::Compare(super::compare::Ask::parse(line)),
        Some("settags") => Request::SetTags {
            rows: field_usize_array(line, "rows"),
            op: field_str(line, "op").unwrap_or_default(),
            tag: field_str(line, "tag").unwrap_or_default(),
        },
        Some("tagstates") => Request::TagStates { rows: field_usize_array(line, "rows"), id: field_usize(line, "id").unwrap_or(0) },
        Some("devs") => Request::Devs { paths: field_str_array(line, "paths"), id: field_usize(line, "id").unwrap_or(0) },
        Some("jump") => Request::Jump { id: field_usize(line, "id").unwrap_or(0), ranking: field_usize(line, "ranking").unwrap_or(0), favourites: field_str_array(line, "favourites"), recent: field_str_array(line, "recent") },
        Some("clipSet") => Request::ClipSet {
            op: field_str(line, "op").unwrap_or_default(),
            paths: field_str_array(line, "paths"),
        },
        Some("clipGet") => Request::ClipGet,
        Some("clipClear") => Request::ClipClear {
            token: field_str(line, "token").unwrap_or_default(),
            cut: field_str_array(line, "cut"),
        },
        Some("clipWatch") => Request::ClipWatch,
        Some(c) if c.starts_with("flash") => super::flash::parse(c, line).map_or(Request::Unknown, Request::Flash),
        Some("quit") => Request::Quit,
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

pub fn listed_line(n: usize, read_ms: f64, sort_ms: f64, dev: u64, path: &str, writable: bool) -> String {
    say_listed(n, read_ms, sort_ms, dev, path, writable)
}

// A session answer's listed line, carrying the query gen so a late line lands on no folder.
pub fn listed_gen_line(n: usize, read_ms: f64, sort_ms: f64, dev: u64, path: &str, writable: bool, gen: usize) -> String {
    let mut line = say_listed(n, read_ms, sort_ms, dev, path, writable);
    if line.ends_with('}') {
        line.insert_str(line.len() - 1, &format!(",\"gen\":{}", gen));
    }
    line
}

// `w` is whether this user can create or delete entries here; a drag from one that cannot copies.
pub(crate) fn say_listed(n: usize, read_ms: f64, sort_ms: f64, dev: u64, path: &str, writable: bool) -> String {
    format!(
        r#"{{"t":"listed","n":{},"read":{:.3},"sort":{:.3},"v":{},"w":{},"path":"{}"}}"#,
        n, read_ms, sort_ms, dev, writable, escape(path)
    )
}

// A listed line with the anchor fields added before its closing brace.
pub(crate) fn with_anchor(listed: &str, anchor: &str, anchor_index: isize) -> String {
    let body = listed.strip_suffix('}').unwrap_or(listed);
    format!(r#"{},"anchor":"{}","anchorIndex":{}}}"#, body, escape(anchor), anchor_index)
}

// Sample output: {"t":"listed","n":3,"read":0.000,"sort":0.000,"v":1,"w":true,"path":"/d","changed":2}
// A same-path re-list names added plus removed rows, so a rename counts 2 against a net delta of 0.
pub(crate) fn with_changed(listed: &str, changed: usize) -> String {
    let body = listed.strip_suffix('}').unwrap_or(listed);
    format!(r#"{},"changed":{}}}"#, body, changed)
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

// Sample output: {"t":"slow","op":"rename","path":"/hung/a.txt","msg":"/hung is slow. The rename will finish on its own."}
pub fn slow_line(op: &str, path: &str, msg: &str) -> String {
    format!(r#"{{"t":"slow","op":"{}","path":"{}","msg":"{}"}}"#, escape(op), escape(path), escape(msg))
}

// Sample output: {"t":"linked","ok":2,"failed":0,"skipped":1}
// Sample output: {"t":"linked","ok":1,"failed":1,"skipped":0,"note":"the link left at /d/b.txt could not be removed (stale); the replaced item stays in the trash"}
pub fn linked_line(ok: usize, failed: usize, skipped: usize, note: &str) -> String {
    if note.is_empty() {
        return format!(r#"{{"t":"linked","ok":{},"failed":{},"skipped":{}}}"#, ok, failed, skipped);
    }
    format!(r#"{{"t":"linked","ok":{},"failed":{},"skipped":{},"note":"{}"}}"#, ok, failed, skipped, escape(note))
}

// Sample output: {"t":"linktarget","path":"/a/link","directory":"/b","name":"f.txt","id":3}
pub fn linktarget_line(path: &str, directory: &str, name: &str, id: usize) -> String {
    format!(r#"{{"t":"linktarget","path":"{}","directory":"{}","name":"{}","id":{}}}"#,
        escape(path), escape(directory), escape(name), id)
}

// Sample output: {"t":"permissions","id":7,"op":"applyMany","ok":true,"mode":"0600","error":""}
pub fn permissions_batch_line(id: usize, ok: bool, mode: &str, error: &str) -> String {
    format!(r#"{{"t":"permissions","id":{},"op":"applyMany","ok":{},"mode":"{}","error":"{}"}}"#,
        id, ok, escape(mode), escape(error))
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

#[cfg(test)]
#[path = "proto_tests.rs"]
mod tests;
