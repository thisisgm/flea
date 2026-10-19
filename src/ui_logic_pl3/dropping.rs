//! TrashDrop.js: only rows lifted in this window may trash through the rail's Trash row, by path or by lift-time row.
use crate::i18n::I18n;

/// What the drag's own marker says, read by the drag owner's marker parser; None for a foreign or malformed drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Marker<'a> {
    /// The raw marker text, whose row field this module reads.
    pub raw: &'a str,
    /// The folder the drag was lifted from.
    pub source: &'a str,
    /// The listing numbering at the lift, 0 when the lift had none.
    pub listing: u64,
    /// This window's backend recorded this exact marker at the lift.
    pub lifted_here: bool,
}

/// The trash request a drop sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Carried paths survive listing changes.
    Paths(Vec<String>),
    /// A selection too wide to carry paths, named in the numbering it was lifted in, so a stale index is refused.
    Rows { rows: Vec<usize>, listing: u64 },
}

impl Request {
    pub fn line(&self) -> String {
        match self {
            Self::Paths(paths) => format!(r#"{{"c":"trash","paths":{}}}"#, crate::json::string_array(paths)),
            Self::Rows { rows, listing } => {
                let rows = rows.iter().map(usize::to_string).collect::<Vec<_>>().join(",");
                format!(r#"{{"c":"trash","rows":[{rows}],"listing":{listing}}}"#)
            }
        }
    }
}

/// The marker's row field; any malformed entry refuses the whole field.
pub fn marker_rows(marker: &str) -> Vec<usize> {
    let field = marker.split('\n').nth(1).unwrap_or("");
    if field.is_empty() {
        return Vec::new();
    }
    let mut rows = Vec::new();
    for part in field.split(',') {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return Vec::new();
        }
        match part.parse() {
            Ok(row) => rows.push(row),
            Err(_) => return Vec::new(),
        }
    }
    rows
}

/// The request or None: a foreign drag, an untrashable source, or rows that are not this listing's.
pub fn request(marker: Option<Marker>, paths: &[String], pane_path: &str, trashable: impl Fn(&str) -> bool) -> Option<Request> {
    let marker = marker?;
    let source = if marker.source.is_empty() { pane_path } else { marker.source };
    if !trashable(source) {
        return None;
    }
    if !paths.is_empty() {
        return Some(Request::Paths(paths.to_vec()));
    }
    let rows = marker_rows(marker.raw);
    if rows.is_empty() || marker.listing == 0 || marker.source != pane_path || !marker.lifted_here {
        return None;
    }
    Some(Request::Rows { rows, listing: marker.listing })
}

/// Where the cursor lands once the rows are gone: the lowest lifted row, only for a drag from this listing.
pub fn trashed_first(marker: Option<Marker>, pane_path: &str) -> Option<usize> {
    let marker = marker.filter(|m| m.source == pane_path && m.lifted_here)?;
    marker_rows(marker.raw).into_iter().min()
}

/// The status line while a drag rests on Trash, counting the marker's rows before its paths; a refused drag says nothing.
pub fn line(i18n: &I18n, request: Option<&Request>, marker: &str) -> String {
    let Some(request) = request else { return String::new() };
    let rows = marker_rows(marker).len();
    let count = match request {
        _ if rows > 0 => rows,
        Request::Paths(paths) => paths.len(),
        Request::Rows { rows, .. } => rows.len(),
    };
    i18n.trn("trash.drop.line", count as i64)
}
