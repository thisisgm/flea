// One request line from the window, parsed into its Request; an unknown command is Request::Unknown, a missing field its default.
use super::{Request, TRANSFER_CANCEL};
use crate::json::{field_bool, field_str, field_str_array, field_usize, field_usize_array};

// Sample input: {"c":"list","path":"/home/gm","first":350,"hidden":false}
pub fn parse_request(line: &str) -> Request {
    match field_str(line, "c").as_deref() {
        Some("outline") => Request::Outline {
            // Sample input: {"c":"outline","rows":[0,4],"open":true,"anchor":7,"listing":3,"first":320}; a lone "row" still names one folder.
            rows: match field_usize_array(line, "rows") { rows if rows.is_empty() => field_usize(line, "row").into_iter().collect(), rows => rows },
            anchor: field_usize(line, "anchor"), open: field_bool(line, "open"),
            enabled: crate::jsondoc::parse(line).ok().and_then(|doc| doc.get("enabled").and_then(crate::jsondoc::Json::as_bool)), first: field_usize(line, "first").unwrap_or(crate::backend::searchsession::FIRST_WINDOW_DEFAULT),
        },
        Some("nearby") => Request::Nearby {
            op: field_str(line, "op").unwrap_or_default(),
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("networkaddress") => Request::NetworkAddress {
            address: field_str(line, "address").unwrap_or_default(),
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("gitstatus" | "gitrows" | "gitchanges") => Request::Git {
            line: line.to_string(),
        },
        Some("trashbrowse") => Request::TrashBrowse {
            line: line.to_string(),
        },
        Some("plugins") => Request::Plugins {
            line: line.to_string(),
        },
        Some("permissions") => Request::Permissions {
            line: line.to_string(),
        },
        Some("permissionsBatch") => Request::PermissionsBatch {
            paths: field_str_array(line, "paths"),
            modes: field_str_array(line, "modes"),
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("link") => Request::Link {
            held: crate::jsondoc::parse(line).ok().and_then(|v| v.get("held").cloned()),
            // Anything that is not "absolute" or "hard" is a relative link, never an absolute path by accident.
            op: field_str(line, "op").unwrap_or_default(),
            paths: field_str_array(line, "paths"),
            rows: field_usize_array(line, "rows"),
            dest: field_str(line, "dest").unwrap_or_default(),
            collide: crate::backend::collide::Ask::parse(line),
        },
        Some("linktarget") => Request::LinkTarget {
            expected: field_str(line, "target"),
            path: field_str(line, "path").unwrap_or_default(),
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("picker") => Request::Picker {
            line: line.to_string(),
        },
        Some("menuaction") => Request::MenuAction {
            line: line.to_string(),
            rows: field_usize_array(line, "rows"),
        },
        Some("rclone") => Request::Rclone {
            line: line.to_string(),
        },
        Some("jobs") => Request::Jobs {
            line: line.to_string(),
        },
        Some("actions") => Request::Actions {
            line: line.to_string(),
        },
        Some("localsend") => Request::LocalSend {
            op: field_str(line, "op").unwrap_or_default(),
            peer: field_str(line, "peer").unwrap_or_default(),
            paths: field_str_array(line, "paths"),
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("airdrop") => Request::AirDrop {
            peer: field_str(line, "peer").unwrap_or_default(),
            name: field_str(line, "name").unwrap_or_default(),
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
            // Absent keeps only the listed folder's own sizes, so an older client still gets same-folder reuse.
            keep: field_str_array(line, "keep"),
        },
        Some("listpaths") => Request::ListPaths {
            paths: field_str_array(line, "paths"),
            first: field_usize(line, "first").unwrap_or(0),
        },
        Some("window") => Request::Window {
            start: field_usize(line, "start").unwrap_or(0),
            count: field_usize(line, "count").unwrap_or(0),
        },
        Some("filter") => Request::Filter {
            query: field_str(line, "query").unwrap_or_default(),
            id: field_usize(line, "id").unwrap_or(0),
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
        Some("searchcontents") => Request::SearchContents {
            line: line.to_owned(),
        },
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
        Some("thumb") => match crate::backend::thumbreq::identity::Tag::parse(line) {
            Ok(identity) => Request::Thumb { rows: field_usize_array(line, "rows"), cache_only: field_bool(line, "cacheOnly"), identity },
            Err(_) => Request::Unknown,
        },
        Some("thumbcancel") => Request::ThumbCancel {
            rows: field_usize_array(line, "rows"),
        },
        Some("trashthumb") => Request::TrashThumb {
            line: line.to_string(),
        },
        Some("trashthumbcancel") => Request::TrashThumbCancel {
            ids: field_usize_array(line, "ids"),
        },
        Some("dirsize") => Request::DirSize {
            rows: field_usize_array(line, "rows"),
        },
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
            collide: crate::backend::collide::Ask::parse(line),
        },
        Some(word @ ("collisions" | "mergecollisions")) => Request::Collisions {
            id: field_usize(line, "id").unwrap_or(0),
            paths: field_str_array(line, "paths"),
            rows: field_usize_array(line, "rows"),
            dest: field_str(line, "dest").unwrap_or_default(),
            menu_id: field_usize(line, "menuId").unwrap_or(0),
            merge: word == "mergecollisions",
            fat: crate::backend::fatnames::Choice::parse(line),
        },
        Some("pasteimage") => Request::PasteImage(crate::backend::pasteimage::Ask::parse(line)),
        Some(TRANSFER_CANCEL) => Request::TransferCancel {
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("trash") => Request::Trash {
            paths: field_str_array(line, "paths"),
            rows: field_usize_array(line, "rows"),
            menu_id: field_usize(line, "menuId").unwrap_or(0),
            request: field_usize(line, "id").unwrap_or(0),
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
        Some("duplicate") => Request::Duplicate {
            path: field_str(line, "path").unwrap_or_default(),
            menu_id: field_usize(line, "menuId").unwrap_or(0),
        },
        Some("mkdir") => Request::MkDir {
            path: field_str(line, "path").unwrap_or_default(),
            name: field_str(line, "name").unwrap_or_default(),
            default_base: field_str(line, "defaultBase"),
        },
        Some("newfile") => Request::NewFile {
            path: field_str(line, "path").unwrap_or_default(),
            name: field_str(line, "name").unwrap_or_default(),
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("undo") => Request::Undo,
        Some("redo") => Request::Redo,
        Some("paths") => match field_usize(line, "images") {
            Some(start) => Request::ImagePaths(crate::backend::viewerpaths::Ask {
                start,
                rows: field_bool(line, "scoped").then(|| field_usize_array(line, "rows")),
                selected: field_bool(line, "selected"),
            }),
            None => Request::Paths {
                rows: field_usize_array(line, "rows"),
            },
        },
        Some("locate") => match field_str(line, "path") {
            Some(path) => Request::Locate { path },
            None => Request::LocateMany {
                paths: field_str_array(line, "paths"),
                id: field_usize(line, "id").unwrap_or(0),
                menu_id: field_usize(line, "menuId").unwrap_or(0),
                transfer_id: field_usize(line, "transferId").unwrap_or(0),
            },
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
        Some("printoptions" | "print" | "convertbatch" | "combine") => {
            Request::PrintConvert(crate::backend::printreq::Request::parse(line))
        }
        Some("convert") => Request::Convert {
            request_id: field_usize(line, "requestId").unwrap_or(0),
            check: field_bool(line, "check"),
            menu_id: field_usize(line, "menuId").unwrap_or(0),
            path: field_str(line, "path").unwrap_or_default(),
            dest: field_str(line, "dest").unwrap_or_default(),
            strip: field_bool(line, "strip"),
        },
        Some("formats") => Request::Formats {
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("fontlook") => Request::FontLook(crate::backend::fontinfo::render::Ask::parse(line)),
        Some("pdfcopy") => Request::PdfCopy {
            id: field_usize(line, "id").unwrap_or(0),
            // Absent is one viewer, so an older client's fetch keeps the shared slot.
            slot: field_str(line, "slot").unwrap_or_default(),
            path: field_str(line, "path").unwrap_or_default(),
        },
        Some(c @ ("officepdf" | "officesheets")) => Request::OfficePdf {
            id: field_usize(line, "id").unwrap_or(0),
            path: field_str(line, "path").unwrap_or_default(),
            sheets: c == "officesheets",
        },
        Some("peek") => Request::Peek {
            details: field_bool(line, "details"),
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
        Some("owner") => Request::Owner {
            row: field_usize(line, "row").unwrap_or(0),
            token: field_usize(line, "token").unwrap_or(0),
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
        Some("code") => Request::Code(crate::backend::codereq::Ask::parse(line)),
        Some("html") => Request::Html(crate::backend::htmlreq::Ask::parse(line)),
        Some("model3d") => Request::Model3d(crate::backend::model3dreq::Ask::parse(line)),
        Some("compare") => Request::Compare(crate::backend::compare::Ask::parse(line)),
        Some("settags") => Request::SetTags {
            rows: field_usize_array(line, "rows"),
            op: field_str(line, "op").unwrap_or_default(),
            tag: field_str(line, "tag").unwrap_or_default(),
        },
        Some("tagstates") => Request::TagStates {
            rows: field_usize_array(line, "rows"),
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("devs") => Request::Devs {
            paths: field_str_array(line, "paths"),
            id: field_usize(line, "id").unwrap_or(0),
        },
        Some("jump") => Request::Jump {
            id: field_usize(line, "id").unwrap_or(0),
            ranking: field_usize(line, "ranking").unwrap_or(0),
            favourites: field_str_array(line, "favourites"),
            recent: field_str_array(line, "recent"),
        },
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
        Some(c) if c.starts_with("flash") => {
            crate::backend::flash::parse(c, line).map_or(Request::Unknown, Request::Flash)
        }
        Some("quit") => Request::Quit,
        _ => Request::Unknown,
    }
}
