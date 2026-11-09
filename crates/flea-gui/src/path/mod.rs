//! The path bar's meaning, the line a typist writes and what it resolves to, with no window and no backend.
pub(crate) mod aliases;
pub mod chrome;
pub mod crumbs;
pub mod entry;
pub mod field;
mod focus;
pub mod history;
mod input;
pub mod join;
#[cfg(test)]
mod m9_g2rows_29_netpath_tests;
pub mod mark;
pub mod menu;
pub mod paint;
mod probe;
pub mod rank;
pub mod render;
pub mod scale;
pub mod strip;
mod strip_input;
pub mod suggestions;
pub(crate) mod tap;
pub mod view;
mod wire;

use crate::pane::Kind;
use flea::i18n::I18n;
use flea::ui_logic_network_shares::networkaddress;
pub use wire::{answer, is_jump_line, jump_line, peek_line, Answer, Jumped, Peeked};

// The one place a path is made canonical: "." and ".." resolve here, so the bar draws the path the backend lists.
pub fn normalize(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    let mut out = String::with_capacity(path.len() + 1);
    for part in &parts {
        out.push('/');
        out.push_str(part);
    }
    if out.is_empty() {
        out.push('/');
    }
    out
}

// RFC 8089 spells this machine two ways; any other authority names a file elsewhere, which this bar cannot open.
const LOCAL_HOSTS: [&str; 2] = ["", "localhost"];

// Sample input: "file://localhost/etc" is "/etc", "file://otherbox/etc" is "", "/etc" stays itself.
pub fn unwrap(text: &str) -> String {
    let Some(body) = text.strip_prefix("file://") else {
        return text.to_owned();
    };
    let (authority, rest) = match body.find('/') {
        Some(cut) => (&body[..cut], &body[cut..]),
        None => (body, "/"),
    };
    if !LOCAL_HOSTS.contains(&authority.to_lowercase().as_str()) {
        return String::new();
    }
    decode(rest).unwrap_or_else(|| rest.to_owned())
}

// decodeURIComponent: each run of %XX must be whole UTF-8, or the line is kept as it stands.
fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut run = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = text.get(at + 1..at + 3)?;
            run.push(
                u8::from_str_radix(hex, 16)
                    .ok()
                    .filter(|_| hex.bytes().all(|b| b.is_ascii_hexdigit()))?,
            );
            at += 3;
            continue;
        }
        if !run.is_empty() {
            out.push_str(std::str::from_utf8(&run).ok()?);
            run.clear();
        }
        let next = text[at..].chars().next()?;
        out.push(next);
        at += next.len_utf8();
    }
    out.push_str(std::str::from_utf8(&run).ok()?);
    Some(out)
}

fn trim(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

// A typed smb:// (or sftp://, davs://, ...) or user@host:/path is a network location, handed back whole for the network open.
pub fn is_network_uri(text: &str) -> bool {
    let line = trim(text);
    let uri = ["smb", "sftp", "ftp", "ftps", "dav", "davs", "nfs", "afp"]
        .iter()
        .any(|scheme| {
            line.len() >= scheme.len() + 3
                && line.is_char_boundary(scheme.len())
                && line[..scheme.len()].eq_ignore_ascii_case(scheme)
                && line[scheme.len()..].starts_with("://")
        });
    // A slash before the "@" names a local path ("./a@b:/c", "/srv/a@b:/c"), as scp reads it.
    uri || (line
        .split_once('@')
        .is_some_and(|(user, _)| !user.contains('/'))
        && networkaddress::parse(line).is_ok())
}

// The typed line as an absolute path, or "" for a line that names nothing.
pub fn resolve(text: &str, current: &str, home: &str) -> String {
    let line = trim(text);
    if is_network_uri(line) {
        return line.to_owned();
    }
    let body = unwrap(line);
    if body.is_empty() {
        return String::new();
    }
    let body = if !home.is_empty() && (body == "~" || body.starts_with("~/")) {
        format!("{home}{}", &body[1..])
    } else if !body.starts_with('/') {
        format!("{current}/{body}")
    } else {
        body
    };
    normalize(&body)
}

// The line cut at its last slash: head names a directory, leaf is the half name being typed.
pub fn split(text: &str) -> (&str, &str) {
    match text.rfind('/') {
        Some(cut) => (&text[..=cut], &text[cut + 1..]),
        None => ("", text),
    }
}

// The directory a completion reads: the head alone, or the pane's own directory for a bare leaf.
pub fn completion_dir(text: &str, current: &str, home: &str) -> String {
    match split(trim(text)).0 {
        "" => current.to_owned(),
        head => resolve(head, current, home),
    }
}

// A dotted leaf asks for hidden entries whatever the listing shows.
pub fn wants_hidden(text: &str, show_hidden: bool) -> bool {
    show_hidden || split(trim(text)).1.starts_with('.')
}

/// What a completion request and its peeked reply are matched on: two peeks of one directory and flag answer alike.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ask {
    pub dir: String,
    pub hidden: bool,
}

pub fn request_key(dir: &str, hidden: bool) -> Ask {
    Ask {
        dir: dir.to_owned(),
        hidden,
    }
}

pub fn common_prefix<S: AsRef<str>>(names: &[S]) -> String {
    let Some(first) = names.first() else {
        return String::new();
    };
    let mut prefix = first.as_ref();
    for name in &names[1..] {
        let shared = prefix
            .char_indices()
            .zip(name.as_ref().chars())
            .find(|((_, a), b)| a != b)
            .map_or(prefix.len().min(name.as_ref().len()), |((at, _), _)| at);
        let shared = (0..=shared)
            .rev()
            .find(|cut| prefix.is_char_boundary(*cut))
            .unwrap_or(0);
        prefix = &prefix[..shared];
    }
    prefix.to_owned()
}

pub fn starts_with(name: &str, leaf: &str, fold: bool) -> bool {
    if fold {
        name.to_lowercase().starts_with(&leaf.to_lowercase())
    } else {
        name.starts_with(leaf)
    }
}

/// The line Tab leaves and how many entries stand behind it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub text: String,
    pub matches: usize,
}

// Case-sensitively first, and folded only when that matched nothing: "Dev" and "dev" are two directories.
pub fn complete<S: AsRef<str>>(text: &str, names: &[S]) -> Completion {
    let (head, leaf) = split(text);
    let pick = |fold| -> Vec<&str> {
        names
            .iter()
            .map(AsRef::as_ref)
            .filter(|name| starts_with(name, leaf, fold))
            .collect()
    };
    let mut matched = pick(false);
    if matched.is_empty() {
        matched = pick(true);
    }
    if matched.is_empty() {
        return Completion {
            text: text.to_owned(),
            matches: 0,
        };
    }
    let mut grown = common_prefix(&matched);
    // A fold can share less than was typed ("re" against README and ReadMe.bak), and the line never shrinks.
    if grown.chars().count() < leaf.chars().count() {
        grown = leaf.to_owned();
    }
    let tail = if matched.len() == 1 { "/" } else { "" };
    Completion {
        text: format!("{head}{grown}{tail}"),
        matches: matched.len(),
    }
}

// Sample input: should_navigate("/etc", "/etc", false) is false, the settled no-op that keeps the selection.
pub fn should_navigate(target: &str, current: &str, failed: bool) -> bool {
    !target.is_empty() && (target != current || failed)
}

// Nav.pathFailed: only a settled error or locked listing retries; a search, Trash or a listing in flight never does.
pub fn failed(kind: Kind, in_flight: bool, search: bool, trash: bool) -> bool {
    !in_flight && !search && !trash && matches!(kind, Kind::Error | Kind::Locked)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeekKind {
    Folder,
    Missing,
    File,
}

const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;

// Sample input: (false, 0) and (true, 16877) are Folder, (true, 0) Missing, (true, 33188) File.
pub fn kind_of_peek(read_failed: bool, mode: u32) -> PeekKind {
    if !read_failed || mode & S_IFMT == S_IFDIR {
        PeekKind::Folder
    } else if mode == 0 {
        PeekKind::Missing
    } else {
        PeekKind::File
    }
}

// A line that named something yet resolved to nothing owes a sentence; an empty line owes silence.
pub fn refused(text: &str) -> bool {
    let line = trim(text);
    !line.is_empty() && unwrap(line).is_empty()
}

/// Every sentence the path bar says, resolved through the catalog when it is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Said {
    NothingStartsWith(String),
    ManyStart(usize),
    RefusedUri,
    Missing(String),
    Filtered(String),
    NetworkUnavailable,
}

impl Said {
    pub fn text(&self, catalog: &I18n) -> String {
        match self {
            Self::NothingStartsWith(dir) => {
                catalog.tr_args("path.nothing_starts_with", &[dir.as_str()])
            }
            Self::ManyStart(count) => catalog.trn("path.items_start_with", *count as i64),
            Self::RefusedUri => catalog.tr("path.refused_uri"),
            Self::Missing(path) => catalog.tr_args("path.missing", &[path.as_str()]),
            Self::Filtered(path) => catalog.tr_args("path.filtered", &[path.as_str()]),
            Self::NetworkUnavailable => catalog.tr("path.network_unavailable"),
        }
    }
}

// What Tab says when it changed nothing; None is the usual, silent case.
pub fn completion_message(before: &str, after: &Completion, dir: &str) -> Option<Said> {
    if after.matches == 0 {
        Some(Said::NothingStartsWith(dir.to_owned()))
    } else if after.text == before {
        Some(Said::ManyStart(after.matches))
    } else {
        None
    }
}
