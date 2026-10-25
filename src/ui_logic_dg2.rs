//! Native file drag and drop decisions shared by every front end, ported from ui/js/Drag.js, DragOut.js and RailDrop.js; no I/O, the host turns each answer into a platform offer or a backend request.
use crate::i18n::I18n;
pub use crate::ui_logic_dg1::{LinkKind, PasteAsk, PLAIN, PLAIN_UTF8, URILIST};
use crate::ui_logic_dg1::{build_plain, build_urilist};

#[cfg(test)]
mod tests;

/// Flea's own drag marker type, offered beside the uri-list (Drag.js ROWS_MIME).
pub const ROWS_MIME: &str = "application/x-flea-rows";

pub const INSIDE_DRAG: &str = "drag.inside";
pub const ALREADY_HERE: &str = "drag.already_here";
pub const NO_LOCAL_FILES: &str = "drag.no_local_files";
pub const MOVE_TO_FOLDER: &str = "drag.move_to_folder";
pub const COPY_TO_FOLDER: &str = "drag.copy_to_folder";
pub const LINK_TO_FOLDER: &str = "drag.link_to_folder";
pub const TOO_WIDE: &str = "drag.too_wide";

/// The rows a drag carries: the selection when the pressed row is in it, else the pressed row alone.
pub fn carried(selected: &[usize], index: usize) -> Vec<usize> {
    if selected.contains(&index) {
        selected.to_vec()
    } else {
        vec![index]
    }
}

/// What a lift bakes into the drag: sender, rows, lift keys, source folder and its device, deletability and listing numbering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub instance: String,
    pub rows: Vec<usize>,
    pub copy: bool,
    pub source: String,
    pub dev: u64,
    pub deletable: bool,
    pub shift: bool,
    pub listing: u64,
}

impl Marker {
    // Sample output: "me\n1,3\nmove\n%2Fhome%2Fu\n42\n1\n0\n7", the eight-field form Drag.markerPayload writes.
    pub fn encode(&self) -> String {
        let rows = self.rows.iter().map(usize::to_string).collect::<Vec<_>>().join(",");
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.instance,
            rows,
            if self.copy { "copy" } else { "move" },
            encode_component(&self.source),
            self.dev,
            if self.deletable { "1" } else { "0" },
            if self.shift { "1" } else { "0" },
            self.listing
        )
    }

    // Five- and six-field legacy markers carry a raw source; seven and eight percent-encode it; anything else is None.
    pub fn parse(payload: &str) -> Option<Self> {
        let fields: Vec<&str> = payload.split('\n').collect();
        if !matches!(fields.len(), 5..=8) {
            return None;
        }
        let dev = fields[4];
        let signed = dev.strip_prefix('-').unwrap_or(dev);
        if signed.is_empty() || !signed.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let copy = match fields[2] {
            "copy" => true,
            "move" => false,
            _ => return None,
        };
        let flag = |at: usize| match fields.get(at) {
            None => Some(None),
            Some(&"0") => Some(Some(false)),
            Some(&"1") => Some(Some(true)),
            Some(_) => None,
        };
        let deletable = flag(5)?.unwrap_or(true);
        let shift = flag(6)?.unwrap_or(false);
        let source = if fields.len() >= 7 { decode_component(fields[3])? } else { fields[3].to_owned() };
        let listing = match fields.get(7) {
            None => 0,
            Some(text) if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => text.parse().ok()?,
            Some(_) => return None,
        };
        // A negative or unreadable device is unknown, as Number(...) || 0 reads it.
        let dev = if dev.starts_with('-') { 0 } else { dev.parse().unwrap_or(0) };
        Some(Self {
            instance: fields[0].to_owned(),
            rows: fields[1].split(',').filter_map(|row| row.parse().ok()).collect(),
            copy,
            source,
            dev,
            deletable,
            shift,
            listing,
        })
    }
}

/// Whether a marker text is absent or well formed; a malformed one refuses every drop (Drag.canDropInto).
pub fn marker_ok(marker: Option<&str>) -> bool {
    // map_or because is_none_or needs Rust 1.82 over the 1.77 floor.
    marker.map_or(true, |text| Marker::parse(text).is_some())
}

// encodeURIComponent: unreserved ASCII and !'()*~ stay, every other byte is %XX.
fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// decodeURIComponent: a bad escape or invalid UTF-8 is None.
fn decode_component(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = std::str::from_utf8(bytes.get(at + 1..at + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A path as a file:// URI, each component encoded on its own (Drag.uriFor).
pub fn uri_for(path: &str) -> String {
    let parts: Vec<String> = path.split('/').map(encode_component).collect();
    format!("file://{}", parts.join("/"))
}

/// Sample input "file:///d/a%20b.txt" answers "/d/a b.txt"; another host or scheme answers None (DragOut.filePath).
pub fn file_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let path = if rest.starts_with('/') {
        rest
    } else {
        let slash = rest.find('/')?;
        if &rest[..slash] != "localhost" {
            return None;
        }
        &rest[slash..]
    };
    decode_component(path)
}

pub fn file_paths(urls: &[String]) -> Vec<String> {
    urls.iter().filter_map(|url| file_path(url)).collect()
}

/// The uri-list lines of a drop, comments and blanks left out; the paths come from file_paths.
pub fn uri_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Sample input "/a/x.txt\nhttp://e/y\n" answers ["/a/x.txt"] (DragOut.plainPaths).
pub fn plain_paths(plain: &str) -> Vec<String> {
    plain
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| line.starts_with('/'))
        .map(str::to_owned)
        .collect()
}

/// The local paths a drop carries; plain text adds lines only to a drag without Flea's marker (DragOut.sources).
pub fn sources(urls: &[String], plain: Option<&str>, has_marker: bool) -> Vec<String> {
    let mut paths = file_paths(urls);
    if !has_marker {
        for extra in plain.map(plain_paths).unwrap_or_default() {
            if !paths.contains(&extra) {
                paths.push(extra);
            }
        }
    }
    paths
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
    if paths.iter().any(|path| dest == path || dest.strip_prefix(path.as_str()).is_some_and(|rest| rest.starts_with('/'))) {
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
pub fn drop_verb(marker: Option<&Marker>, proposed: Proposed, dest_dev: u64, ctrl: Seen, shift: Seen) -> Verb {
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
    let items = i18n.trn("items.count", count as i64);
    let key = match (copy, shift) {
        (true, true) => LINK_TO_FOLDER,
        (true, false) => COPY_TO_FOLDER,
        _ => MOVE_TO_FOLDER,
    };
    i18n.tr_args(TOO_WIDE, &[&i18n.tr_args(key, &[&items])])
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
pub fn drop_into(dropped: &Dropped<'_>, dest: &str, dest_dev: u64) -> Result<PasteAsk, Option<Refusal>> {
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
    Ok(match drop_verb(dropped.marker, dropped.proposed, dest_dev, dropped.ctrl, dropped.shift) {
        Verb::Link => PasteAsk::Link { kind: LinkKind::Relative, paths, dest: dest.to_owned() },
        verb => PasteAsk::Transfer {
            op: if verb == Verb::Move { "move" } else { "copy" }.to_owned(),
            paths,
            dest: dest.to_owned(),
            spends_cut: false,
        },
    })
}

/// What kind of rail row a drop rests on; only a folder row can be a destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailKind {
    Folder,
    Other,
}

// Sample input: "/run/media/gm/USB/" is "/run/media/gm/USB" (DropDev.normalize).
fn normalize(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() && path.starts_with('/') {
        "/"
    } else {
        trimmed
    }
}

// A phone root under gvfs takes no drop.
fn phone_root(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/run/user/") else {
        return false;
    };
    let Some((uid, rest)) = rest.split_once('/') else {
        return false;
    };
    let Some(name) = rest.strip_prefix("gvfs/") else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    !uid.is_empty()
        && uid.bytes().all(|b| b.is_ascii_digit())
        && ["mtp:", "gphoto2:", "afc:"].iter().any(|scheme| lower.starts_with(scheme) && lower.len() > scheme.len())
        && !name.contains('/')
}

/// The folder a rail row takes a drop into, or "" when it takes none (RailDrop.destination).
pub fn rail_destination(kind: RailKind, path: &str, mounted: bool, urls: &[String], plain: Option<&str>, has_marker: bool) -> String {
    if kind != RailKind::Folder || !mounted {
        return String::new();
    }
    let path = normalize(path);
    if !path.starts_with('/') || phone_root(path) {
        return String::new();
    }
    let paths = sources(urls, plain, has_marker);
    if paths.is_empty() || refusal(&paths, path, has_marker).is_some() {
        return String::new();
    }
    path.to_owned()
}
