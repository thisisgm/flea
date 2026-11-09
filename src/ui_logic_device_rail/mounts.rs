//! The gio mount listing's grammar, rail keys and the rail's shared row rules.
use super::menu::rail_menu;
use super::protocols::{share_name, strip_default_port};
use super::{Group, Kind, RailEntry};

/// One top-level "Mount(N): name -> uri" line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountLine {
    pub name: String,
    pub uri: String,
}

/// A labelled uri: a network mount or a saved bookmark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub label: String,
    pub uri: String,
}

/// What gio adds to a kernel mount row, matched by path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelMount {
    pub path: String,
    pub label: String,
    pub uri: String,
}

/// Every top-level mount line of gio mount -l.
pub fn mount_lines(output: &str) -> Vec<MountLine> {
    output.split('\n').filter_map(mount_line).collect()
}

fn mount_line(line: &str) -> Option<MountLine> {
    let rest = line.strip_prefix("Mount(")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let rest = rest[digits..].strip_prefix("):").filter(|_| digits > 0)?;
    let start = rest.len() - rest.trim_start().len();
    let arrows: Vec<usize> = rest.match_indices("->").map(|(at, _)| at).collect();
    let valid = |at: usize| {
        let mut tail = rest[at + 2..].split_whitespace();
        tail.next().is_some() && tail.next().is_none()
    };
    let (name, at) = match arrows.iter().copied().find(|&at| at > start && valid(at)) {
        Some(at) => (rest[start..at].trim_end().to_string(), at),
        None => {
            let at = arrows.iter().copied().find(|&at| at == start && start > 0 && valid(at))?;
            (rest[..start].chars().last()?.to_string(), at)
        }
    };
    let uri = rest[at + 2..].split_whitespace().next()?.to_string();
    Some(MountLine { name, uri })
}

/// Network mounts only: a local file:// mount and a phone's shadow line are other groups' rows.
pub fn parse_mounts(output: &str) -> Vec<Place> {
    mount_lines(output)
        .into_iter()
        .filter(|m| !m.uri.starts_with("file://") && !is_phone_uri(&m.uri))
        .map(|m| Place { label: share_name(&m.name, &m.uri), uri: m.uri })
        .collect()
}

fn is_phone_uri(uri: &str) -> bool {
    let lower = uri.to_lowercase();
    ["mtp://", "gphoto2://", "afc://"].iter().any(|p| lower.starts_with(p))
}

/// The file:// lines of a gio listing, path decoded and its trailing slashes dropped.
pub fn gio_kernel_mounts(output: &str) -> Vec<KernelMount> {
    let mut out = Vec::new();
    for m in mount_lines(output) {
        let Some(raw) = m.uri.strip_prefix("file://") else { continue };
        let decoded = decode_path(raw);
        let path = decoded.trim_end_matches('/');
        let path = if path.is_empty() { "/".to_string() } else { path.to_string() };
        out.push(KernelMount { path, label: share_name(&m.name, &m.uri), uri: m.uri });
    }
    out
}

/// Percent-decoding that answers the raw text for a malformed escape or invalid UTF-8.
pub fn decode_path(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'%' {
            out.push(bytes[at]);
            at += 1;
            continue;
        }
        let hex = raw.get(at + 1..at + 3).filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
        let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) else { return raw.to_string() };
        out.push(byte);
        at += 3;
    }
    String::from_utf8(out).unwrap_or_else(|_| raw.to_string())
}

/// Whether a folder can be trashed: never read-only, never inside a gvfs FUSE mount.
pub fn trashable(path: &str, writable: Option<bool>) -> bool {
    writable != Some(false) && !under_gvfs(path)
}

/// A path inside /run/user/<uid>/gvfs/<scheme>:..., where no backend implements trash.
pub fn under_gvfs(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/run/user/") else { return false };
    let Some(slash) = rest.find('/').filter(|s| *s > 0) else { return false };
    let Some(mount) = rest[slash..].strip_prefix("/gvfs/") else { return false };
    let word = mount.bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'-').count();
    word > 0 && mount[word..].starts_with(':')
}

fn scheme_lead(text: &str) -> Option<usize> {
    let mark = text.find("://")?;
    let head = &text[..mark];
    let ok = head.starts_with(|c: char| c.is_ascii_alphabetic())
        && head.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
    ok.then_some(mark + 3)
}

/// The bookmarks file's non-file lines as places, a bare line labelled by its leaf.
pub fn non_file_bookmarks(body: &str) -> Vec<Place> {
    let mut out = Vec::new();
    for line in body.split('\n').map(str::trim) {
        if line.is_empty() || line.starts_with("file://") {
            continue;
        }
        let (uri, label) = line.split_once(' ').map_or((line, ""), |(u, l)| (u, l.trim()));
        if scheme_lead(uri).is_none() {
            continue;
        }
        let label = if label.is_empty() { leaf(uri) } else { label.to_string() };
        out.push(Place { uri: uri.to_string(), label });
    }
    out
}

/// The last path segment, or the host for a bare root.
pub fn leaf(uri: &str) -> String {
    let stripped = scheme_lead(uri).map_or(uri, |at| &uri[at..]).trim_end_matches('/');
    let tail = stripped.rfind('/').map_or(stripped, |slash| &stripped[slash + 1..]);
    if tail.is_empty() { stripped.to_string() } else { tail.to_string() }
}

/// One canonical uri: trailing slashes gone, a bare host root keeping one, the default port dropped.
pub fn normalize(uri: &str) -> String {
    let stripped = strip_default_port(uri.trim_end_matches('/'));
    let bare_root = scheme_lead(&stripped).is_some_and(|at| at < stripped.len() && !stripped[at..].contains('/'));
    if bare_root { stripped + "/" } else { stripped }
}

/// NETWORK shows on its own discoveries, DEVICES only after it; the deadline shows both.
pub fn rail_groups_ready(network_done: bool, devices_done: bool, elapsed_ms: u64, deadline_ms: u64) -> (bool, bool) {
    if elapsed_ms >= deadline_ms {
        return (true, true);
    }
    (network_done, network_done && devices_done)
}

/// Whether two rail lists draw the same, so an unchanged poll assigns nothing.
pub fn same_entries(a: &[RailEntry], b: &[RailEntry]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same_entry(x, y))
}

fn same_entry(x: &RailEntry, y: &RailEntry) -> bool {
    RailEntry { uuid: String::new(), ..x.clone() } == RailEntry { uuid: String::new(), ..y.clone() }
}

/// A row's stable handle: a volume's device node, a phone's or share's uri, "" for any other row.
pub fn rail_key(entry: &RailEntry) -> String {
    match (entry.group, entry.kind) {
        (Group::Device, Kind::Volume) => entry.device.clone(),
        (Group::Device, Kind::Phone) | (Group::Network, Kind::Share) => entry.uri.clone(),
        _ => String::new(),
    }
}

/// Where a key resolves in a list, None for an empty key or a row that left.
pub fn row_by_key(entries: &[RailEntry], key: &str) -> Option<usize> {
    if key.is_empty() {
        return None;
    }
    entries.iter().position(|e| rail_key(e) == key)
}

/// The device or network row with a release whose folder holds dir.
pub fn holding<'a>(entries: &'a [RailEntry], dir: &str) -> Option<&'a RailEntry> {
    entries.iter().find(|e| {
        let grouped = matches!(e.group, Group::Device | Group::Network);
        !e.path.is_empty() && grouped && !rail_menu(e).is_empty() && inside(dir, &e.path)
    })
}

/// Whether dir is root itself or under it.
pub fn inside(dir: &str, root: &str) -> bool {
    dir == root || dir.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}

/// What a poll does with a fresh listing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollDecision {
    None,
    Skip,
    Rebuild,
}

/// Skip when both the listing and mountinfo are unchanged; nothing before the first listing.
pub fn poll_decision(listed_once: bool, polled: &str, last: &str, mountinfo: &str, last_mountinfo: &str) -> PollDecision {
    if !listed_once {
        return PollDecision::None;
    }
    if polled == last && mountinfo == last_mountinfo { PollDecision::Skip } else { PollDecision::Rebuild }
}
