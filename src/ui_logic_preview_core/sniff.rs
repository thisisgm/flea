//! Bounded local-file head sniffing for preview hosts; callers run the I/O on a worker.
use std::{
    io::Read,
    os::unix::io::AsRawFd,
    path::{Component, Path, PathBuf},
};

/// open(2)'s O_NOCTTY, the same value on x86_64 and aarch64 (asm-generic).
const O_NOCTTY: i32 = 0o400;

/// The bytes of a file's head that are judged.
const HEAD_BYTES: usize = 4096;

/// Symlinks followed before a path is refused, the kernel's own MAXSYMLINKS.
const MAX_HOPS: usize = 40;

/// What a file's head read as: not text, text, or a script by its `#!` line, which previews as code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Head {
    Binary,
    Text,
    Script,
}

impl Head {
    /// What a preview mounts for this head: `text` for text, `script` for a `#!` script, nothing for the rest.
    pub fn pick<T>(self, text: T, script: T) -> Option<T> {
        match self {
            Head::Binary => None,
            Head::Text => Some(text),
            Head::Script => Some(script),
        }
    }
}

/// Sample input: `/mnt/My\040Drive` gives `/mnt/My Drive`.
/// The mount table's octal escapes (space, tab, newline, backslash) decoded.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let octal = bytes.get(at + 1..at + 4).filter(|digits| {
            bytes[at] == b'\\' && digits.iter().all(|digit| (b'0'..=b'7').contains(digit))
        });
        match octal {
            Some(digits) => {
                out.push(
                    digits
                        .iter()
                        .fold(0u8, |value, digit| value * 8 + (digit - b'0')),
                );
                at += 4;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A filesystem that may answer slowly: network, FUSE, or an automount not yet triggered.
fn slow(kind: &str) -> bool {
    crate::backend::netfs::can_wedge(kind) || kind == "autofs"
}

/// Sample input: a /proc/self/mounts line, `rc: /mnt/My\040Drive fuse.rclone rw 0 0` (source, point, type).
/// Whether the mount holding this path answers slowly. Purely lexical, so asking never touches the mount itself.
fn remote(table: &str, path: &Path) -> bool {
    table
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let (_, point, kind) = (fields.next()?, fields.next()?, fields.next()?);
            let point = PathBuf::from(unescape(point));
            path.starts_with(&point)
                .then(|| (point.components().count(), kind.to_owned()))
        })
        .max_by_key(|(depth, _)| *depth)
        .map_or(true, |(_, kind)| slow(&kind))
}

/// Every symlink followed one component at a time, each step checked against the mount table first, so a link into a remote mount is refused.
fn resolve_local(table: &str, path: &Path) -> Option<PathBuf> {
    let mut pending: Vec<PathBuf> = path
        .components()
        .rev()
        .map(|part| PathBuf::from(part.as_os_str()))
        .collect();
    let mut resolved = PathBuf::from("/");
    let mut hops = 0;
    while let Some(part) = pending.pop() {
        match Path::new(&part).components().next()? {
            Component::RootDir => resolved = PathBuf::from("/"),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                let candidate = resolved.join(name);
                if remote(table, &candidate) {
                    return None;
                }
                let meta = std::fs::symlink_metadata(&candidate).ok()?;
                if !meta.file_type().is_symlink() {
                    resolved = candidate;
                    continue;
                }
                hops += 1;
                if hops > MAX_HOPS {
                    return None;
                }
                let target = std::fs::read_link(&candidate).ok()?;
                pending.extend(
                    target
                        .components()
                        .rev()
                        .map(|part| PathBuf::from(part.as_os_str())),
                );
            }
            Component::Prefix(_) => return None,
        }
    }
    Some(resolved)
}

/// Sample input: /proc/self/fdinfo/7 holds `pos: 0`, `flags: 0104000`, `mnt_id: 57`, one per line.
/// The filesystem type of the mount an open descriptor lives on, from its mount id, whatever path reached it.
fn descriptor_kind(fd: i32, mountinfo: &str) -> Option<String> {
    let fdinfo = std::fs::read_to_string(format!("/proc/self/fdinfo/{fd}")).ok()?;
    let id = fdinfo
        .lines()
        .find_map(|line| line.strip_prefix("mnt_id:"))?
        .trim();
    mount_kind(mountinfo, id)
}

/// Sample input: `57 21 0:50 / /mnt/d rw - fuse.rclone rc: rw`, the id first and the type after ` - `.
/// The filesystem type of the mount with this id in /proc/self/mountinfo.
fn mount_kind(mountinfo: &str, id: &str) -> Option<String> {
    mountinfo.lines().find_map(|line| {
        let (head, tail) = line.split_once(" - ")?;
        (head.split_whitespace().next()? == id)
            .then(|| tail.split_whitespace().next().map(str::to_owned))
            .flatten()
    })
}

/// A regular local file's head judged; None when it could not be read this time, which is not kept, so a later ask reads again.
pub fn read_head(path: &str) -> Option<Head> {
    if !path.starts_with('/') {
        return Some(Head::Binary);
    }
    let table = std::fs::read_to_string("/proc/self/mounts").ok()?;
    let path = resolve_local(&table, Path::new(path))?;
    // Only a regular file is opened: a device or tty row is refused before any open, which could have side effects.
    match std::fs::symlink_metadata(&path) {
        Ok(stat) if stat.is_file() => {}
        Ok(_) => return Some(Head::Binary),
        Err(_) => return None,
    }
    // A fifo, device or link swapped in after that check opens without blocking or following, and its descriptor's type refuses it.
    let (mut file, meta) = match crate::backend::regfile::open_if_regular_with_meta(
        &path,
        crate::oflags::O_NOFOLLOW | O_NOCTTY,
    ) {
        Ok(opened) => opened,
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {
            return Some(Head::Binary)
        }
        Err(_) => return None,
    };
    if meta.len() == 0 {
        return Some(Head::Binary);
    }
    // A directory swapped in above the file could have led the open onto a slow mount; the descriptor says which.
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    if slow(&descriptor_kind(file.as_raw_fd(), &mountinfo)?) {
        return Some(Head::Binary);
    }
    let mut head = [0u8; HEAD_BYTES];
    let read = file.read(&mut head).ok().filter(|read| *read > 0)?;
    Some(judge(&head[..read], meta.len() > read as u64))
}

/// Text by the rule the previews decode with, a script when it starts `#!`; `cut` says the file goes on past the head.
fn judge(head: &[u8], cut: bool) -> Head {
    if !crate::textdecode::looks_text(head, cut) {
        Head::Binary
    } else if head.starts_with(b"#!") {
        Head::Script
    } else {
        Head::Text
    }
}

#[cfg(test)]
#[path = "sniff_tests.rs"]
mod tests;
