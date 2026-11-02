//! Native file drag and drop decisions shared by every front end, ported from ui/js/Drag.js, DragOut.js and RailDrop.js; no I/O, the host turns each answer into a platform offer or a backend request.
use crate::i18n::I18n;
use crate::ui_logic_dg1::{build_plain, build_urilist};
pub use crate::ui_logic_dg1::{LinkKind, PasteAsk, PLAIN, PLAIN_UTF8, URILIST};

mod payload;
mod rail;

pub use payload::{
    file_path, file_paths, marker_ok, plain_paths, sources, uri_for, uri_lines, Marker,
};
pub use rail::{rail_destination, RailKind};

#[cfg(test)]
mod tests;

/// Flea's own drag marker type, offered beside the uri-list (Drag.js ROWS_MIME).
pub const ROWS_MIME: &str = "application/x-flea-rows";

/// The drop shelf's drag type (DragOut rule 4): the single-use token it minted, then the intent fixed at the lift, one per line.
pub const SHELF_MIME: &str = "application/x-flea-shelf";

/// A shelf drag's token and whether its intent copies (Drag.shelfToken, shelfCopying); None when it carries no token.
/// Sample input: "opaque-token\ncopy"
pub fn shelf(payload: &str) -> Option<(&str, bool)> {
    let mut lines = payload.split('\n');
    let token = lines.next().filter(|token| !token.is_empty())?;
    Some((token, lines.next() == Some("copy")))
}

pub const INSIDE_DRAG: &str = "drag.inside";
pub const ALREADY_HERE: &str = "drag.already_here";
pub const NO_LOCAL_FILES: &str = "drag.no_local_files";
pub const MOVE_TO_FOLDER: &str = "drag.move_to_folder";
pub const COPY_TO_FOLDER: &str = "drag.copy_to_folder";
pub const LINK_TO_FOLDER: &str = "drag.link_to_folder";
pub const TOO_WIDE: &str = "drag.too_wide";
pub const MOVE_TO: &str = "drag.move_to";
pub const COPY_TO: &str = "drag.copy_to";
pub const LINK_TO: &str = "drag.link_to";

/// The rows a drag carries: the selection when the pressed row is in it, else the pressed row alone.
pub fn carried(selected: &[usize], index: usize) -> Vec<usize> {
    if selected.contains(&index) {
        selected.to_vec()
    } else {
        vec![index]
    }
}

/// The frozen refusal sentences of DragOut.refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    InsideDrag,
    AlreadyHere,
    NoLocalFiles,
}

impl Refusal {
    pub fn key(self) -> &'static str {
        match self {
            Self::InsideDrag => INSIDE_DRAG,
            Self::AlreadyHere => ALREADY_HERE,
            Self::NoLocalFiles => NO_LOCAL_FILES,
        }
    }

    pub fn say(self, i18n: &I18n) -> String {
        i18n.tr(self.key())
    }
}

// The folder holding a path; a top-level entry's parent is "/".
fn parent(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(at) => &path[..at],
    }
}

/// Checked in order: a folder inside the drag, a file already in the destination, then nothing local.
pub fn refusal(paths: &[String], dest: &str, has_marker: bool) -> Option<Refusal> {
    if paths.iter().any(|path| {
        dest == path
            || dest
                .strip_prefix(path.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    }) {
        return Some(Refusal::InsideDrag);
    }
    if paths.iter().any(|path| parent(path) == dest) {
        return Some(Refusal::AlreadyHere);
    }
    (paths.is_empty() && !has_marker).then_some(Refusal::NoLocalFiles)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Copy,
    Move,
    Link,
}

/// The only verb decision: Ctrl with Shift links, Ctrl copies, Shift moves; else same device moves and unknown device or undeletable copies.
pub fn verb_for(ctrl: bool, shift: bool, src_dev: u64, dest_dev: u64, deletable: bool) -> Verb {
    match (ctrl, shift) {
        (true, true) => Verb::Link,
        (true, false) => Verb::Copy,
        (false, true) => Verb::Move,
        _ if src_dev == 0 || dest_dev == 0 || !deletable => Verb::Copy,
        _ if src_dev == dest_dev => Verb::Move,
        _ => Verb::Copy,
    }
}

/// A modifier seen during the drag: held or released at the drop, or never seen so the lift bit stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Seen {
    #[default]
    Unseen,
    Held,
    Released,
}

pub fn drop_held(lift: bool, seen: Seen) -> bool {
    match seen {
        Seen::Held => true,
        Seen::Released => false,
        Seen::Unseen => lift,
    }
}

/// The actions a foreign source proposes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Proposed {
    pub copy: bool,
    pub move_: bool,
    pub link: bool,
}

/// (ctrl, shift) a marker-less drag stands for: link only links, move only shifts, copy only copies, else plain.
pub fn foreign_held(proposed: Proposed) -> (bool, bool) {
    match (proposed.copy, proposed.move_, proposed.link) {
        (false, false, true) => (true, true),
        (false, true, _) => (false, true),
        (true, false, _) => (true, false),
        _ => (false, false),
    }
}

/// Verb from the marker plus drop-time keys; a foreign drag follows foreign_held with an unknown source device.
pub fn drop_verb(
    marker: Option<&Marker>,
    proposed: Proposed,
    dest_dev: u64,
    ctrl: Seen,
    shift: Seen,
) -> Verb {
    match marker {
        Some(marker) => verb_for(
            drop_held(marker.copy, ctrl),
            drop_held(marker.shift, shift),
            marker.dev,
            dest_dev,
            marker.deletable,
        ),
        None => {
            let (ctrl, shift) = foreign_held(proposed);
            verb_for(ctrl, shift, 0, dest_dev, true)
        }
    }
}

/// What a lift offers: the marker always, the uri-list and plain paths only when every carried row resolved (Drag.mimeFor).
pub fn lift_payload(marker: &Marker, paths: Option<&[String]>) -> Vec<(&'static str, Vec<u8>)> {
    let mut out = vec![(ROWS_MIME, marker.encode().into_bytes())];
    if let Some(paths) = paths.filter(|paths| !paths.is_empty()) {
        out.push((URILIST, build_urilist(paths)));
        out.push((PLAIN, build_plain(paths)));
    }
    out
}

/// The one platform action a lift offers, so any receiver takes the lift's verb (FileDrag.qml supportedActions): plain and Ctrl copy, Shift moves; a Ctrl+Shift link has no platform action here and offers copy.
pub fn offer(copy: bool, shift: bool) -> (bool, bool, Verb) {
    if shift && !copy {
        (false, true, Verb::Move)
    } else {
        (true, false, Verb::Copy)
    }
}

/// The pane message for a lift whose selection reaches past the rows the window holds (FileDrag.cannotLeave): the drag line with no folder name and the reach note.
pub fn too_wide(i18n: &I18n, count: usize, copy: bool, shift: bool) -> String {
    i18n.tr_args(TOO_WIDE, &[&line_to_folder(i18n, count, copy, shift)])
}

/// The status line while a lift rests on no folder (Drag.line with no name, FileDrag.startOffer): the verb its keys ask for and the count.
pub fn line_to_folder(i18n: &I18n, count: usize, copy: bool, shift: bool) -> String {
    let items = i18n.trn("items.count", count as i64);
    let key = match (copy, shift) {
        (true, true) => LINK_TO_FOLDER,
        (true, false) => COPY_TO_FOLDER,
        _ => MOVE_TO_FOLDER,
    };
    i18n.tr_args(key, &[&items])
}

/// The status line while a drag rests on a named folder (Drag.line with a name): the verb, the count and the folder.
pub fn line_to(i18n: &I18n, count: usize, name: &str, verb: Verb) -> String {
    let items = i18n.trn("items.count", count as i64);
    let key = match verb {
        Verb::Link => LINK_TO,
        Verb::Copy => COPY_TO,
        Verb::Move => MOVE_TO,
    };
    i18n.tr_args(key, &[&items, name])
}

/// The board's own words on the hovered folder (Drag.label): move here, copy here or link here.
pub fn drop_label(i18n: &I18n, verb: Verb) -> String {
    i18n.tr(match verb {
        Verb::Link => "grid.drop.link",
        Verb::Copy => "grid.drop.copy",
        Verb::Move => "grid.drop.move",
    })
}

/// One drop as the destination read it.
#[derive(Clone, Copy, Debug)]
pub struct Dropped<'a> {
    pub marker: Option<&'a Marker>,
    pub urls: &'a [String],
    pub plain: Option<&'a str>,
    pub proposed: Proposed,
    pub ctrl: Seen,
    pub shift: Seen,
}

/// The transfer or link a drop asks op1 for, or why it lands nowhere: a refusal to say, or None for a silent refusal (Drag.dropInto).
pub fn drop_into(
    dropped: &Dropped<'_>,
    dest: &str,
    dest_dev: u64,
) -> Result<PasteAsk, Option<Refusal>> {
    if dest.is_empty() {
        return Err(None);
    }
    let paths = sources(dropped.urls, dropped.plain, dropped.marker.is_some());
    if let Some(refused) = refusal(&paths, dest, dropped.marker.is_some()) {
        return Err(Some(refused));
    }
    if paths.is_empty() {
        return Err(None);
    }
    Ok(
        match drop_verb(
            dropped.marker,
            dropped.proposed,
            dest_dev,
            dropped.ctrl,
            dropped.shift,
        ) {
            Verb::Link => PasteAsk::Link {
                kind: LinkKind::Relative,
                paths,
                dest: dest.to_owned(),
            },
            verb => PasteAsk::Transfer {
                op: if verb == Verb::Move { "move" } else { "copy" }.to_owned(),
                paths,
                dest: dest.to_owned(),
                spends_cut: false,
            },
        },
    )
}
