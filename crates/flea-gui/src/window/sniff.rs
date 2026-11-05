//! Whether a file with no known type reads as text: its first 4 KiB, read off the UI thread.
//!
//! One read runs at a time, and a newer ask waits for it in place of any older waiting one, so a stalled disk
//! holds one background read rather than the window or a growing queue. A file is read only on a local mount:
//! every component is checked against the mount table before it is looked at, the open neither blocks nor
//! follows a final link, and the open descriptor's own type and mount decide before a byte is read.
use flea::{protocol::RowFacts, ui_logic_pv1::kinds};
use gpui::App;
use std::{
    cell::RefCell,
    collections::HashMap,
    io::Read,
    os::unix::io::AsRawFd,
    path::{Component, Path, PathBuf},
};

/// open(2)'s O_NOCTTY, the same value on x86_64 and aarch64 (asm-generic).
const O_NOCTTY: i32 = 0o400;

/// Answers kept before the cache starts over.
const KEPT: usize = 256;

/// A file as listed: its path, size and modification time, so a changed file is read again.
pub(crate) type Key = (String, u64, i64);

type Done = Box<dyn FnOnce(&mut App)>;

#[derive(Default)]
struct Sniffer {
    answers: HashMap<Key, bool>,
    running: Option<(Key, Vec<Done>)>,
    waiting: Option<(Key, Vec<Done>)>,
}

thread_local! {
    static SNIFFER: RefCell<Sniffer> = RefCell::default();
}

/// Only a regular row the backend could name no type for, and never a font, which Quick Look opens itself.
pub(crate) fn candidate(row: &RowFacts) -> bool {
    !row.dir && row.kind == kinds::UNKNOWN_KIND && !kinds::is_font(&row.name.to_ascii_lowercase())
}

pub(crate) fn key(row: &RowFacts, path: &str) -> Key {
    (path.to_owned(), row.size, row.mtime)
}

/// The answer for this file as listed, None until a read has given one.
pub(crate) fn answer(key: &Key) -> Option<bool> {
    SNIFFER.with(|sniffer| sniffer.borrow().answers.get(key).copied())
}

/// Reads the file's head in the background; `done` runs on the UI thread once the read is over, whether or not
/// it kept an answer, and never for an ask a newer one replaced while it waited.
pub(crate) fn ask(key: Key, cx: &mut App, done: impl FnOnce(&mut App) + 'static) {
    let done: Done = Box::new(done);
    let start = SNIFFER.with(|sniffer| {
        let mut sniffer = sniffer.borrow_mut();
        let sniffer = &mut *sniffer;
        if sniffer.answers.contains_key(&key) {
            return None;
        }
        match (&mut sniffer.running, &mut sniffer.waiting) {
            (Some((running, list)), _) if *running == key => list.push(done),
            (Some(_), Some((waiting, list))) if *waiting == key => list.push(done),
            (Some(_), waiting) => *waiting = Some((key, vec![done])),
            (None, _) => {
                sniffer.running = Some((key.clone(), vec![done]));
                return Some(key);
            }
        }
        None
    });
    if let Some(key) = start {
        run(key, cx);
    }
}

fn run(key: Key, cx: &mut App) {
    let path = key.0.clone();
    let task = cx
        .background_executor()
        .spawn(async move { looks_text(&path) });
    cx.spawn(async move |cx| {
        let text = task.await;
        let _ = cx.update(|cx| landed(key, text, cx));
    })
    .detach();
}

fn landed(key: Key, text: Option<bool>, cx: &mut App) {
    let (done, next) = SNIFFER.with(|sniffer| {
        let mut sniffer = sniffer.borrow_mut();
        if sniffer.answers.len() >= KEPT {
            sniffer.answers.clear();
        }
        // A read that failed keeps no answer, so the next ask for this file reads it again.
        if let Some(text) = text {
            sniffer.answers.insert(key, text);
        }
        let done = sniffer
            .running
            .take()
            .map(|(_, done)| done)
            .unwrap_or_default();
        sniffer.running = sniffer.waiting.take();
        (done, sniffer.running.as_ref().map(|(key, _)| key.clone()))
    });
    if let Some(next) = next {
        run(next, cx);
    }
    for done in done {
        done(cx);
    }
}

/// A preview showing a row with no known type draws again once its head is read, so it can mount it as text.
pub(crate) fn notify_when_read<T: 'static>(row: &RowFacts, path: &str, cx: &mut gpui::Context<T>) {
    let key = key(row, path);
    if candidate(row) && answer(&key).is_none() {
        let this = cx.entity().downgrade();
        ask(key, cx, move |cx| {
            let _ = this.update(cx, |_, cx| cx.notify());
        });
    }
}

impl crate::state::Shell {
    /// Quick Look opened an unread row as it is; a head that reads as text reopens it as text while the card
    /// still shows that row, the open's token telling.
    pub(super) fn quick_look_sniff(
        &mut self,
        key: Key,
        token: u64,
        (side, index): (usize, usize),
        row: RowFacts,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let (this, handle, path) = (
            cx.entity().downgrade(),
            window.window_handle(),
            key.0.clone(),
        );
        let asked = key.clone();
        ask(key, cx, move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                this.update(cx, |shell, cx| {
                    let text = answer(&asked) == Some(true);
                    let current = text
                        && shell
                            .quick_look
                            .as_ref()
                            .is_some_and(|host| host.token == token);
                    if current && shell.quick_look_open(cx) {
                        shell.show_row(side, index, &row, path, window, cx);
                    }
                })
            });
        });
    }
}

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
    flea::backend::netfs::can_wedge(kind) || kind == "autofs"
}

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
        .is_none_or(|(_, kind)| slow(&kind))
}

/// The path with every symlink followed one component at a time, checking each step against the mount
/// table before it is looked at, so a link into a remote mount is refused rather than followed there.
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
                if hops > 40 {
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

/// The filesystem type of the mount an open descriptor lives on, from its mount id, whatever path reached it.
fn descriptor_kind(fd: i32, mountinfo: &str) -> Option<String> {
    let fdinfo = std::fs::read_to_string(format!("/proc/self/fdinfo/{fd}")).ok()?;
    let id = fdinfo
        .lines()
        .find_map(|line| line.strip_prefix("mnt_id:"))?
        .trim();
    mount_kind(mountinfo, id)
}

/// The filesystem type of the mount with this id in /proc/self/mountinfo.
fn mount_kind(mountinfo: &str, id: &str) -> Option<String> {
    mountinfo.lines().find_map(|line| {
        let (head, tail) = line.split_once(" - ")?;
        (head.split_whitespace().next()? == id)
            .then(|| tail.split_whitespace().next().map(str::to_owned))
            .flatten()
    })
}

/// The first 4 KiB of a regular local file: no NUL and valid UTF-8, where only a character the
/// 4 KiB cut through may be incomplete. None when it could not be read this time (no permission, gone, a
/// failed read), which is not kept, so a later ask reads it again.
pub(crate) fn looks_text(path: &str) -> Option<bool> {
    if !path.starts_with('/') {
        return Some(false);
    }
    let table = std::fs::read_to_string("/proc/self/mounts").ok()?;
    let path = resolve_local(&table, Path::new(path))?;
    // Only a regular file is opened: a device or tty row is refused before any open, which could have side effects.
    match std::fs::symlink_metadata(&path) {
        Ok(stat) if stat.is_file() => {}
        Ok(_) => return Some(false),
        Err(_) => return None,
    }
    // A fifo or device swapped in after that check opens without blocking, never as a controlling terminal, and is
    // refused by the descriptor's type; a link swapped in is not followed.
    let (mut file, meta) = match flea::backend::regfile::open_if_regular_with_meta(
        &path,
        flea::oflags::O_NOFOLLOW | O_NOCTTY,
    ) {
        Ok(opened) => opened,
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => return Some(false),
        Err(_) => return None,
    };
    if meta.len() == 0 {
        return Some(false);
    }
    // A directory swapped in above the file could have led the open onto a slow mount; the descriptor says which.
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    if slow(&descriptor_kind(file.as_raw_fd(), &mountinfo)?) {
        return Some(false);
    }
    let mut head = [0u8; 4096];
    let read = file.read(&mut head).ok().filter(|read| *read > 0)?;
    let head = &head[..read];
    if head.contains(&0) {
        return Some(false);
    }
    Some(match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(error) => error.error_len().is_none() && meta.len() > read as u64,
    })
}

#[cfg(test)]
#[path = "sniff_tests.rs"]
mod tests;
