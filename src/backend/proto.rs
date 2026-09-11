use crate::error::FleaError;
use crate::json::{escape, field_bool, field_str, field_str_array, field_usize, field_usize_array};

pub enum Request {
    List { path: String, first: usize, hidden: bool },
    // A listing built from paths the client names, in the order it named them; the picker's Recent.
    ListPaths { paths: Vec<String>, first: usize },
    Window { start: usize, count: usize },
    Sort { by: String, desc: bool },
    Search { path: String, query: String, hidden: bool },
    // Unlike thumbcancel there is no rows form: one walk runs at a time, so a cancel can only mean that one.
    SearchCancel,
    Thumb { rows: Vec<usize> },
    ThumbCancel { rows: Vec<usize> },
    DirSize { rows: Vec<usize> },
    // Unlike thumbcancel, there is no rows form: it always cancels everything in flight, see docs/protocol.md "dirsizecancel".
    DirSizeCancel,
    // The five write operations and their cancel, per the operations design's own wire.
    Transfer { op: String, paths: Vec<String>, rows: Vec<usize>, dest: String, menu_id: usize },
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
    Peek { path: String, first: usize, hidden: bool, focus: String },
    // op is "compress" or "extract"; a compress names paths and a format, an extract names one path.
    Archive { op: String, paths: Vec<String>, path: String, dest: String, format: String, menu_id: usize },
    Convert { path: String, dest: String, strip: bool, menu_id: usize, request_id: usize, check: bool },
    // Which archive formats this box actually offers, and whether a converter is installed at all.
    Formats { id: usize },
    Permissions { line: String },
    Picker { line: String },
    MenuAction { line: String, rows: Vec<usize> },
    TrashBrowse { line: String },
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
        },
        Some("search") => Request::Search {
            path: field_str(line, "path").unwrap_or_default(),
            query: field_str(line, "query").unwrap_or_default(),
            hidden: field_bool(line, "hidden"),
        },
        Some("searchcancel") => Request::SearchCancel,
        Some("thumb") => Request::Thumb { rows: field_usize_array(line, "rows") },
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
        },
        Some("transfercancel") => Request::TransferCancel { id: field_usize(line, "id").unwrap_or(0) },
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
        Some("mkdir") => Request::MkDir {
            path: field_str(line, "path").unwrap_or_default(),
            name: field_str(line, "name").unwrap_or_default(),
        },
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
            focus: field_str(line, "focus").unwrap_or_default(),
        },
        Some("meta") => Request::Meta {
            token: field_usize(line, "token").unwrap_or(0),
            row: field_usize(line, "row").unwrap_or(0),
            text: field_bool(line, "text"),
            media: field_bool(line, "media"),
            archive: field_bool(line, "archive"),
        },
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

pub fn listed_line(n: usize, read_ms: f64, sort_ms: f64, dev: u64) -> String {
    format!(
        r#"{{"t":"listed","n":{},"read":{:.3},"sort":{:.3},"v":{}}}"#,
        n, read_ms, sort_ms, dev
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
mod tests {
    use super::*;

    #[test]
    fn locate_preserves_request_identity_and_reports_absence() {
        assert!(matches!(parse_request(r#"{"c":"locate","path":"/a/file"}"#), Request::Locate { path } if path == "/a/file"));
        assert_eq!(located_line("/a", "/a/file", Some(3)), r#"{"t":"located","directory":"/a","path":"/a/file","index":3}"#);
        assert_eq!(located_line("/a", "/a/\"\n", None), r#"{"t":"located","directory":"/a","path":"/a/\"\n","index":-1}"#);
        assert!(matches!(parse_request(r#"{"c":"locate","paths":["/a/file"],"id":2,"menuId":7}"#),
            Request::LocateMany { paths, id: 2, menu_id: 7, transfer_id: 0 } if paths == ["/a/file"]));
        assert!(matches!(parse_request(r#"{"c":"locate","paths":["/a/file"],"transferId":12}"#),
            Request::LocateMany { paths, id: 0, menu_id: 0, transfer_id: 12 } if paths == ["/a/file"]));
        assert_eq!(located_many_line("/a", 2, 0, &[("/a/\"\n", 3)], None),
            r#"{"t":"located","directory":"/a","id":2,"transferId":0,"matches":[{"path":"/a/\"\n","index":3}],"ok":true,"error":""}"#);
    }

    #[test]
    fn convert_preserves_probe_and_caller_identity_without_changing_legacy_activation() {
        assert!(matches!(parse_request(r#"{"c":"convert","path":"/a.png","dest":"/a.jpg","strip":true,"menuId":7,"requestId":29,"check":true}"#),
            Request::Convert { path, dest, strip: true, menu_id: 7, request_id: 29, check: true } if path == "/a.png" && dest == "/a.jpg"));
        assert!(matches!(parse_request(r#"{"c":"convert","path":"/a.png","dest":"/a.jpg"}"#),
            Request::Convert { strip: false, menu_id: 0, request_id: 0, check: false, .. }));
    }

    #[test]
    fn parses_each_request_shape() {
        match parse_request(r#"{"c":"list","path":"/home/gm","first":350}"#) {
            Request::List { path, first, hidden } => {
                assert_eq!(path, "/home/gm");
                assert_eq!(first, 350);
                assert!(!hidden);
            }
            _ => panic!("expected List"),
        }
        match parse_request(r#"{"c":"window","start":1200,"count":350}"#) {
            Request::Window { start, count } => {
                assert_eq!(start, 1200);
                assert_eq!(count, 350);
            }
            _ => panic!("expected Window"),
        }
        match parse_request(r#"{"c":"sort","by":"size","desc":true}"#) {
            Request::Sort { by, desc } => {
                assert_eq!(by, "size");
                assert!(desc);
            }
            _ => panic!("expected Sort"),
        }
        match parse_request(r#"{"c":"search","path":"/home/gm","query":"bench","hidden":true}"#) {
            Request::Search { path, query, hidden } => {
                assert_eq!(path, "/home/gm");
                assert_eq!(query, "bench");
                assert!(hidden);
            }
            _ => panic!("expected Search"),
        }
        match parse_request(r#"{"c":"mkdir","path":"/home/gm","name":"New Folder"}"#) {
            Request::MkDir { path, name } => assert_eq!((path.as_str(), name.as_str()), ("/home/gm", "New Folder")),
            _ => panic!("expected MkDir"),
        }
        assert!(matches!(parse_request(r#"{"c":"searchcancel"}"#), Request::SearchCancel));
        assert!(matches!(parse_request(r#"{"c":"quit"}"#), Request::Quit));
    }

    #[test]
    fn a_paths_line_escapes_every_element_and_survives_an_empty_list() {
        assert_eq!(paths_line(&[]), r#"{"t":"paths","paths":[]}"#);
        assert_eq!(
            paths_line(&["/home/gm/a.txt".to_string(), "/home/gm/say \"hi\".txt".to_string()]),
            r#"{"t":"paths","paths":["/home/gm/a.txt","/home/gm/say \"hi\".txt"]}"#
        );
    }

    #[test]
    fn junk_is_unknown_rather_than_a_panic() {
        assert!(matches!(parse_request(""), Request::Unknown));
        assert!(matches!(parse_request("not json at all"), Request::Unknown));
        assert!(matches!(parse_request("{"), Request::Unknown));
        assert!(matches!(parse_request(r#"{"c":"nope"}"#), Request::Unknown));
    }

    #[test]
    fn a_malformed_escape_never_panics() {
        // A bad escape only empties that one field; "c" alone decides the variant.
        assert!(matches!(parse_request(r#"{"c":"list","path":"/tmp/a"#), Request::List { .. }));
        assert!(matches!(parse_request(r#"{"c":"list","path":"\u00"#), Request::List { .. }));
        assert!(matches!(parse_request(r#"{"c":"list","path":"\ud800","first":1}"#), Request::List { .. }));
        assert!(matches!(parse_request(r#"{"c":"list","path":"trailing\"#), Request::List { .. }));
        assert!(matches!(parse_request(r#"{"c":"window","start":-5,"count":10}"#), Request::Window { .. }));
        assert!(matches!(parse_request(r#"{"c":"sort","by":"name","desc":truthy}"#), Request::Sort { .. }));
    }

    #[test]
    fn a_list_request_without_first_defaults_to_zero() {
        match parse_request(r#"{"c":"list","path":"/tmp"}"#) {
            Request::List { first, .. } => assert_eq!(first, 0),
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn a_list_request_carries_its_hidden_flag() {
        match parse_request(r#"{"c":"list","path":"/tmp","first":0,"hidden":true}"#) {
            Request::List { hidden, .. } => assert!(hidden),
            _ => panic!("expected List"),
        }
        // Missing and explicitly false both mean dotfiles stay out of the scan.
        match parse_request(r#"{"c":"list","path":"/tmp","first":0}"#) {
            Request::List { hidden, .. } => assert!(!hidden),
            _ => panic!("expected List"),
        }
        match parse_request(r#"{"c":"list","path":"/tmp","first":0,"hidden":false}"#) {
            Request::List { hidden, .. } => assert!(!hidden),
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn emits_a_listed_line() {
        let s = listed_line(100000, 26.4, 2.5, 56);
        assert_eq!(s, r#"{"t":"listed","n":100000,"read":26.400,"sort":2.500,"v":56}"#);
    }

    #[test]
    fn a_thumb_request_carries_its_rows_in_order() {
        match parse_request(r#"{"c":"thumb","rows":[2,17,140]}"#) {
            Request::Thumb { rows } => assert_eq!(rows, vec![2, 17, 140]),
            _ => panic!("expected Thumb"),
        }
        match parse_request(r#"{"c":"thumbcancel","rows":[17,140]}"#) {
            Request::ThumbCancel { rows } => assert_eq!(rows, vec![17, 140]),
            _ => panic!("expected ThumbCancel"),
        }
    }

    #[test]
    fn a_dirsize_request_carries_its_rows_and_cancel_carries_none() {
        match parse_request(r#"{"c":"dirsize","rows":[4,9]}"#) {
            Request::DirSize { rows } => assert_eq!(rows, vec![4, 9]),
            _ => panic!("expected DirSize"),
        }
        // Unlike thumbcancel, dirsizecancel names nothing: it always cancels everything in flight.
        assert!(matches!(parse_request(r#"{"c":"dirsizecancel"}"#), Request::DirSizeCancel));
        assert!(matches!(
            parse_request(r#"{"c":"dirsizecancel","rows":[1,2]}"#),
            Request::DirSizeCancel
        ));
    }

    #[test]
    fn a_thumb_request_with_no_usable_rows_is_still_a_thumb_request() {
        // The empty form of thumbcancel is a documented feature, so a missing or garbled rows must not change the variant.
        for line in [
            r#"{"c":"thumbcancel"}"#,
            r#"{"c":"thumbcancel","rows":[]}"#,
            r#"{"c":"thumbcancel","rows":[-1]}"#,
            r#"{"c":"thumbcancel","rows":nonsense}"#,
        ] {
            match parse_request(line) {
                Request::ThumbCancel { rows } => assert!(rows.is_empty(), "{} should carry no rows", line),
                _ => panic!("expected ThumbCancel for {}", line),
            }
        }
        match parse_request(r#"{"c":"thumb","rows":[1.5,"x",99999999999999999999]}"#) {
            Request::Thumb { rows } => assert!(rows.is_empty()),
            _ => panic!("expected Thumb"),
        }
    }

    #[test]
    fn emits_a_thumbed_line_for_a_generated_row_and_for_a_failed_one() {
        assert_eq!(
            thumbed_line(2, "/home/gm/.cache/thumbnails/large/b98fa408.png", 75.8234),
            r#"{"t":"thumbed","row":2,"file":"/home/gm/.cache/thumbnails/large/b98fa408.png","ms":75.823}"#
        );
        // The empty file is the whole failure form on this wire, so a client never waits forever.
        assert_eq!(thumbed_line(0, "", 0.0), r#"{"t":"thumbed","row":0,"file":"","ms":0.000}"#);
    }

    #[test]
    fn emits_a_dirsized_line_complete_and_partial() {
        assert_eq!(
            dirsized_line(4, 1048576, false, 12.5),
            r#"{"t":"dirsized","row":4,"bytes":1048576,"partial":false,"ms":12.500}"#
        );
        // partial:true is a floor, not a wrong exact number; the cell renders it with a leading ">".
        assert_eq!(
            dirsized_line(9, 200, true, 2000.0),
            r#"{"t":"dirsized","row":9,"bytes":200,"partial":true,"ms":2000.000}"#
        );
    }

    #[test]
    fn a_thumbed_path_is_escaped_like_every_other_string() {
        let s = thumbed_line(7, "/tmp/say \"hi\"\nand\ttab.png", 1.0);
        assert_eq!(s.lines().count(), 1);
        assert!(s.contains(r#""file":"/tmp/say \"hi\"\nand\ttab.png""#));
    }

    #[test]
    fn emits_an_error_line_naming_operation_and_path() {
        let e = FleaError {
            where_: "scan".to_string(),
            path: "/root".to_string(),
            msg: "permission denied".to_string(),
        };
        assert_eq!(
            error_line(&e),
            r#"{"t":"error","where":"scan","path":"/root","msg":"permission denied"}"#
        );
        // The mode rides on the same line, after msg, so an old reader keeps parsing what it knows.
        assert_eq!(
            error_line_with_mode(&e, 0o40750),
            r#"{"t":"error","where":"scan","path":"/root","msg":"permission denied","mode":16872}"#
        );
        // Zero is "I could not stat it either", and that draws no mode string, so it sends no field.
        assert_eq!(
            error_line_with_mode(&e, 0),
            r#"{"t":"error","where":"scan","path":"/root","msg":"permission denied"}"#
        );
    }
}
