#[path = "protoparse.rs"]
mod parse;
pub use parse::parse_request;
#[path = "protoreply.rs"]
mod reply;
pub use reply::{
    dirsized_line, error_line, error_line_with_code, error_line_with_mode, linked_line,
    linktarget_line, listed_gen_line, listed_line, located_line, located_many_line, paths_line,
    permissions_batch_line, searched_line, searching_line, slow_line, thumbed_line,
};
pub(crate) use reply::{say_listed, with_anchor, with_changed};

#[cfg(test)]
use crate::error::FleaError;

// The one spelling of the token the stdin reading path also matches; see src/backend/events.rs.
pub const TRANSFER_CANCEL: &str = "transfercancel";

pub enum Request {
    Rclone {
        line: String,
    },
    Jobs {
        line: String,
    },
    Actions {
        line: String,
    },
    Git {
        line: String,
    },
    Outline { rows: Vec<usize>, anchor: Option<usize>, open: bool, enabled: Option<bool>, first: usize },
    List {
        path: String,
        first: usize,
        hidden: bool,
        want_changed: bool,
    },
    // A listing built from paths the client names, in the order it named them; the picker's Recent.
    ListPaths { paths: Vec<String>, first: usize },
    Window { start: usize, count: usize },
    // A name-only subsequence of the whole listing, carrying original row indices.
    Filter { query: String, id: usize },
    // ordering::request reads desc from the raw line, so this parsed copy is read by tests alone; anchor is the cursor's row.
    Sort { by: String, #[cfg_attr(not(test), allow(dead_code))] desc: bool, anchor: Option<String> },
    Search { path: String, query: String, hidden: bool },
    SearchContents { line: String },
    // Unlike thumbcancel there is no rows form: one walk runs at a time, so a cancel can only mean that one.
    SearchCancel,
    // A live session: the scope walks once, each query answers from the index with its own generation.
    SearchOpen { path: String, hidden: bool, folder: String },
    SearchQuery { query: String, gen: usize, count: Option<usize>, sizes: Option<usize> },
    SearchClose,
    // cacheOnly answers from the shared cache alone and never starts a decoder, for a
    // storage class whose switch is off; absent is today's full path, see docs/protocol.md "thumb".
    Thumb { rows: Vec<usize>, cache_only: bool, identity: Option<super::thumbreq::identity::Tag> },
    ThumbCancel { rows: Vec<usize> },
    TrashThumb { line: String },
    TrashThumbCancel { ids: Vec<usize> },
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
    Trash { paths: Vec<String>, rows: Vec<usize>, menu_id: usize, request: usize },
    Rename { path: String, to: String, menu_id: usize },
    // Bulk rename: parallel paths and bare new names, validated whole before any rename.
    BulkRename { paths: Vec<String>, names: Vec<String> },
    Duplicate { path: String, menu_id: usize },
    // One new empty directory inside parent path; an empty name asks for the first free "New Folder".
    MkDir { path: String, name: String, default_base: Option<String> },
    NewFile { path: String, name: String, id: usize },
    Undo,
    Redo,
    // Resolves row indices to absolute paths, which is what lets a client hold a clipboard for a
    // selection wider than the window it renders; see docs/protocol.md "paths".
    Paths { rows: Vec<usize> },
    ImagePaths(super::viewerpaths::Ask),
    Locate { path: String },
    LocateMany { paths: Vec<String>, id: usize, menu_id: usize, transfer_id: usize },
    // The preview column's own extras for one row: pixels, line count, symlink target.
    Meta { row: usize, text: bool, media: bool, archive: bool, token: usize },
    Owner { row: usize, token: usize },
    // The cursor row's two-byte shebang probe for the Make executable row; see docs/protocol.md "shebang".
    Shebang { path: String, id: usize },
    // The code preview's coloured lines for a file or a text the client holds; the wire lives in codereq.rs.
    Code(super::codereq::Ask),
    Html(super::htmlreq::Ask),
    Model3d(super::model3dreq::Ask),
    // Two regular files answered with their unified diff, or the reason there is none; the wire lives in compare.rs.
    Compare(super::compare::Ask),
    // One tag change across rows of the listing in force: add, remove, toggle or clear.
    SetTags { rows: Vec<usize>, op: String, tag: String },
    // Which of Flea's seven tags those rows all carry or some carry, read only; see docs/protocol.md "tagstates".
    TagStates { rows: Vec<usize>, id: usize },
    // The status bar's filesystem line for the directory the pane is on.
    FsInfo,
    // A read-only look at a directory that is not the current listing; the columns view's ancestors.
    Peek { path: String, first: usize, details: bool, hidden: bool, hidden_last: bool, focus: String, watch: bool, keep: Vec<String>, by: Option<String>, desc: bool },
    // A PDF fetched into a session-private copy under a deadline, so a hung mount never freezes the window.
    PdfCopy { id: usize, slot: String, path: String },
    // An office document converted once to a cached PDF by LibreOffice, or with sheets a spreadsheet to one CSV per sheet.
    OfficePdf { id: usize, path: String, sheets: bool },
    // Quick Look's font page: the facts and three sample images for one font; see src/backend/fontinfo/render.rs.
    FontLook(super::fontinfo::render::Ask),
    // op is "compress" or "extract"; a compress names paths and a format, an extract names one path.
    // password travels this stdin wire and leaves it only on the tool's own stdin pipe, never in argv.
    Archive { op: String, paths: Vec<String>, path: String, dest: String, format: String, menu_id: usize,
              password: String, encrypt_names: bool },
    Convert { path: String, dest: String, strip: bool, menu_id: usize, request_id: usize, check: bool },
    PrintConvert(super::printreq::Request),
    // Which archive formats this box actually offers, and whether a converter is installed at all.
    Formats { id: usize },
    Permissions { line: String },
    // Paste as links: one symlink or hard link per source inside dest; see docs/protocol.md "link".
    Link { held: Option<crate::jsondoc::Json>, op: String, paths: Vec<String>, rows: Vec<usize>, dest: String,
           collide: super::collide::Ask },
    // Show original: where a symlink's target lives, the same path Show in
    // folder uses; see docs/protocol.md "linktarget".
    LinkTarget { path: String, id: usize, expected: Option<String> },
    // Permissions for the whole selection: one Entry holds every path the
    // Apply changed, so one undo restores them all; see docs/protocol.md
    // "permissionsBatch".
    PermissionsBatch { paths: Vec<String>, modes: Vec<String>, id: usize },
    Picker { line: String },
    MenuAction { line: String, rows: Vec<usize> },
    // Directive 71: op is "peers" for the flyout's own list and "send" for the transfer it chooses.
    LocalSend { op: String, peer: String, paths: Vec<String>, id: usize },
    // One AirDrop send to the device named; see docs/protocol.md "airdrop".
    AirDrop { peer: String, paths: Vec<String>, id: usize },
    Nearby { op: String, id: usize },
    Plugins { line: String },
    NetworkAddress { address: String, id: usize },
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

#[cfg(test)]
#[path = "proto_tests.rs"]
mod tests;
