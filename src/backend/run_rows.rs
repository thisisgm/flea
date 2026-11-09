//! The row range each request reads, including outline toggles.
use crate::backend::proto::Request;

// One past the last adopted row, so a request naming only held rows spends nothing.
fn rows_end(rows: &[usize]) -> usize {
    rows.iter()
        .copied()
        .max()
        .map(|last| last.saturating_add(1))
        .unwrap_or(0)
}

// One past the last row a request can read; no wildcard arm, so a new request must classify its rows to compile.
pub(super) fn rows_needed(request: &Request) -> usize {
    match request {
        Request::Window { count: 0, .. } => 0,
        Request::Window { start, count } => start.saturating_add(*count),
        Request::MenuAction { rows, .. }
        | Request::Thumb { rows, .. }
        | Request::DirSize { rows, .. }
        | Request::Transfer { rows, .. }
        | Request::Collisions { rows, .. }
        | Request::Link { rows, .. }
        | Request::Trash { rows, .. }
        | Request::Paths { rows } => rows_end(rows),
        Request::ImagePaths(ask) => ask
            .rows
            .as_ref()
            .map_or(usize::MAX, |rows| rows_end(rows))
            .max(ask.start.saturating_add(1)),
        Request::Meta { row, .. } | Request::Owner { row, .. } => row.saturating_add(1),
        Request::Outline { rows, anchor, .. } => rows_end(rows).max(anchor.map_or(0, |row| row.saturating_add(1))),
        // Tags apply to and read held rows; the bulk rename names paths, never rows.
        Request::SetTags { rows, .. } | Request::TagStates { rows, .. } => rows_end(rows),
        Request::BulkRename { .. } => 0,
        // A path lookup can name any match, and a sort reorders the whole set.
        Request::Locate { .. }
        | Request::LocateMany { .. }
        | Request::Sort { .. }
        | Request::Filter { .. } => usize::MAX,
        // A listing request replaces what it reads; the session lifecycle answers fresh, not from adopted rows.
        Request::List { .. }
        | Request::ListPaths { .. }
        | Request::Search { .. }
        | Request::SearchContents { .. }
        | Request::SearchCancel
        | Request::SearchOpen { .. }
        | Request::SearchQuery { .. }
        | Request::SearchClose => 0,
        // A viewport map, not the listing: these rows index work already queued.
        Request::ThumbCancel { .. } | Request::DirSizeCancel => 0,
        // Operations carry paths or no rows; their rows were resolved by their own earlier ask.
        Request::Rclone { .. }
        | Request::Jobs { .. }
        | Request::Actions { .. }
        | Request::TransferCancel { .. }
        | Request::Rename { .. }
        | Request::Duplicate { .. }
        | Request::MkDir { .. }
        | Request::NewFile { .. }
        | Request::Undo
        | Request::Redo
        | Request::PermissionsBatch { .. }
        | Request::LinkTarget { .. }
        | Request::Archive { .. }
        | Request::Convert { .. }
        | Request::PrintConvert(_)
        | Request::PasteImage(_) => 0,
        // Lines, dialogs and the clipboard name paths or nothing, never a row index.
        Request::Shebang { .. }
        | Request::Code(_)
        | Request::Html(_)
        | Request::Model3d(_)
        | Request::Compare(_)
        | Request::FsInfo
        | Request::Peek { .. }
        | Request::PdfCopy { .. }
        | Request::OfficePdf { .. }
        | Request::FontLook(_)
        | Request::Formats { .. }
        | Request::Permissions { .. }
        | Request::Picker { .. }
        | Request::LocalSend { .. }
        | Request::AirDrop { .. }
        | Request::Nearby { .. }
        | Request::NetworkAddress { .. }
        | Request::Plugins { .. }
        | Request::TrashBrowse { .. }
        | Request::TrashThumb { .. }
        | Request::TrashThumbCancel { .. }
        | Request::Jump { .. }
        | Request::Devs { .. }
        | Request::Flash(_)
        | Request::ClipSet { .. }
        | Request::ClipGet
        | Request::ClipClear { .. }
        | Request::ClipWatch
        | Request::Quit
        | Request::Git { .. }
        | Request::PreviewMeta { .. }
        | Request::Unknown => 0,
    }
}
