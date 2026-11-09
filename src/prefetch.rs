// The page cache prefetch behind the first window: the launcher queues the shell's reads; see AGENTS.md "The first window".
use std::fs;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

use crate::oflags::O_NOFOLLOW;

// The launcher hands the shell the list's path in this variable, and the backend it spawns records into it.
pub const LIST_ENV: &str = "FLEA_PREFETCH";
// The launcher's pid, which exec makes the shell's: only a backend whose parent it is records.
pub const SHELL_ENV: &str = "FLEA_PREFETCH_SHELL";
const HEADER: &str = "flea-prefetch 2";
// A launch that never lists keeps the last list rather than recording whatever it did instead.
const RECORD_TIMEOUT_MS: u64 = 10_000;
// Bounds on a list this process did not write itself: ranges, one range's length, and the list's own size.
const MAX_RANGES: usize = 4096;
const MAX_RANGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_LIST_BYTES: u64 = 1024 * 1024;
// posix_fadvise(2) POSIX_FADV_WILLNEED: queue the reads and return.
const POSIX_FADV_WILLNEED: i32 = 3;
// open(2) O_NONBLOCK, the same on every Linux target: a FIFO swapped in after the check does not block.
const O_NONBLOCK: i32 = 0o4000;
// sysconf(3) _SC_PAGESIZE, the same number on every Linux target the PKGBUILD names.
const SC_PAGESIZE: i32 = 30;
// proc(5) stat fields after the name's closing parenthesis start at field 3, so starttime, field 22, is index 19.
const STAT_STARTTIME_AFTER_NAME: usize = 19;
// pagemap(5): one little-endian u64 per page, bit 63 set when the page is present.
const PAGEMAP_ENTRY_BYTES: u64 = 8;
const PAGEMAP_PRESENT: u64 = 1 << 63;

// std already links the system libc, so the two symbols are declared here rather than taking a crate.
extern "C" {
    fn posix_fadvise(fd: i32, offset: i64, len: i64, advice: i32) -> i32;
    fn sysconf(name: i32) -> i64;
}

// One stretch of one file the shell had in memory: the unit a list is made of.
#[derive(Debug, PartialEq)]
struct Range {
    path: String,
    offset: u64,
    length: u64,
}

// $XDG_CACHE_HOME/flea/prefetch, or None when there is no home to put it in.
pub fn list_path() -> Option<PathBuf> {
    let cache = match crate::userfile::env_dir("XDG_CACHE_HOME") {
        Some(dir) => dir,
        None => crate::userfile::env_dir("HOME")?.join(".cache"),
    };
    Some(cache.join("flea/prefetch"))
}

// The page-cache half the combined launch helper shares: the same bounded read and advice, no fork.
pub(crate) fn do_warm(list: &Path) {
    let Some(text) = read_bounded(list) else { return };
    let mut open: Option<(&str, Option<fs::File>)> = None;
    for range in parse_list(&text) {
        if open.as_ref().map(|(path, _)| *path) != Some(range.path) {
            open = Some((range.path, open_regular(range.path)));
        }
        if let Some((_, Some(file))) = &open {
            // The advice queues and returns; a refusal costs nothing but the read it would have saved.
            unsafe {
                posix_fadvise(file.as_raw_fd(), range.offset as i64, range.length as i64, POSIX_FADV_WILLNEED);
            }
        }
    }
}

// A fifo left at the list's path would block a plain open before the window, so the open is the non-blocking one.
fn read_bounded(list: &Path) -> Option<String> {
    crate::backend::regfile::read_text(list, MAX_LIST_BYTES).ok()
}

// Checked before it is opened, because opening a device node can act on the device.
fn open_regular(path: &str) -> Option<fs::File> {
    if !fs::symlink_metadata(path).ok()?.is_file() {
        return None;
    }
    open_checked(path)
}

// Whatever a swap put at the path since the check: a final symlink fails to open, a FIFO opens without blocking.
fn open_checked(path: &str) -> Option<fs::File> {
    let file = fs::OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_NONBLOCK).open(path).ok()?;
    // Again on the open file, which is the one the advice goes to.
    file.metadata().ok()?.is_file().then_some(file)
}

struct ListRange<'a> {
    path: &'a str,
    offset: u64,
    length: u64,
}

// Sample input: "flea-prefetch 2\nshell 4242 1234567\n0 32768 /usr/lib/libQt6Qml.so.6.11.2\n1048576 4096 /usr/share/fonts/a b.ttf\n"
fn parse_list(text: &str) -> Vec<ListRange<'_>> {
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    for line in lines {
        let mut fields = line.splitn(3, ' ');
        let (Some(offset), Some(length), Some(path)) = (fields.next(), fields.next(), fields.next()) else { continue };
        let (Ok(offset), Ok(length)) = (offset.parse::<u64>(), length.parse::<u64>()) else { continue };
        if !path.starts_with('/') || length == 0 || length > MAX_RANGE_BYTES {
            continue;
        }
        ranges.push(ListRange { path, offset, length });
        if ranges.len() == MAX_RANGES {
            break;
        }
    }
    ranges
}

// Set once by record_after_first_rows and fired by the list handler, so the loop needs no new parameter.
static FIRST_ROWS: OnceLock<mpsc::Sender<()>> = OnceLock::new();

// The backend's side: the shell's pages at the launch's first rows become the next launch's list; AGENTS.md says why then.
pub fn record_after_first_rows() {
    let Some(list) = crate::userfile::env_dir(LIST_ENV) else { return };
    let parent = std::os::unix::process::parent_id();
    // A TUI or test backend under a Flea terminal inherits both variables, but its parent is not the shell.
    if !is_launch_shell(parent, std::env::var(SHELL_ENV).ok().as_deref()) {
        return;
    }
    let (fired, first_rows) = mpsc::channel();
    if FIRST_ROWS.set(fired).is_err() {
        return;
    }
    std::thread::spawn(move || {
        if first_rows.recv_timeout(Duration::from_millis(RECORD_TIMEOUT_MS)).is_err() {
            return;
        }
        // A shell that exited handed this process to a reaper, whose pages say nothing about the shell.
        if std::os::unix::process::parent_id() != parent {
            return;
        }
        let Some(shell) = shell_identity(parent) else { return };
        // A pane or window the shell opens later has first rows of its own, long after the launch's.
        if recorded_by(&list).as_deref() == Some(shell.as_str()) {
            return;
        }
        let Ok(maps) = fs::read_to_string(format!("/proc/{parent}/maps")) else { return };
        let Ok(pagemap) = fs::File::open(format!("/proc/{parent}/pagemap")) else { return };
        let page = page_size();
        let ranges = ranges_from(&maps, page, |start, count| present_pages(&pagemap, start, count, page));
        if !ranges.is_empty() {
            write_list(&list, &shell, &ranges);
        }
    });
}

// Sample input: (4242, Some("4242")) records; a missing, foreign or unparsable pid does not.
fn is_launch_shell(parent: u32, named: Option<&str>) -> bool {
    named.and_then(|pid| pid.parse::<u32>().ok()) == Some(parent)
}

// "shell <pid> <starttime>": the start time tells this shell apart from a later one reusing its pid.
fn shell_identity(pid: u32) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    Some(format!("shell {pid} {}", start_time(&stat)?))
}

// Sample input: "4242 (qs (x) y) S 1 4242 ... 0 5561234 ...", proc(5)'s stat line; the name can hold spaces and parentheses.
fn start_time(stat: &str) -> Option<&str> {
    stat.rsplit_once(')')?.1.split_whitespace().nth(STAT_STARTTIME_AFTER_NAME)
}

// Sample input: "flea-prefetch 2\nshell 4242 5561234\n0 4096 /usr/lib/libc.so.6\n"; the second line, which parse_list skips.
fn recorded_by(list: &Path) -> Option<String> {
    let text = read_bounded(list)?;
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return None;
    }
    lines.next().map(str::to_string)
}

// After every listing's rows: only the first call is heard, and a backend with no list to record hears none.
pub fn first_rows_sent() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(record_after_first_rows);
    if let Some(fired) = FIRST_ROWS.get() {
        let _ = fired.send(());
    }
}

fn page_size() -> u64 {
    let size = unsafe { sysconf(SC_PAGESIZE) };
    // corner: sysconf cannot fail for this name on Linux, and 4 KiB is the smallest page any target has.
    if size > 0 { size as u64 } else { 4096 }
}

// One bit per page of the mapping that starts at `start`, read from the shell's pagemap.
fn present_pages(pagemap: &fs::File, start: u64, count: u64, page: u64) -> Vec<bool> {
    let mut bytes = vec![0u8; (count * PAGEMAP_ENTRY_BYTES) as usize];
    if pagemap.read_exact_at(&mut bytes, start / page * PAGEMAP_ENTRY_BYTES).is_err() {
        return Vec::new();
    }
    bytes
        .chunks_exact(PAGEMAP_ENTRY_BYTES as usize)
        .map(|entry| u64::from_le_bytes(entry.try_into().unwrap_or_default()) & PAGEMAP_PRESENT != 0)
        .collect()
}

// Sample input, one maps line: "7f3c1e200000-7f3c1e5b1000 r--p 00001000 00:1f 1234      /usr/lib/libQt6Qml.so.6.11.2"
fn ranges_from(maps: &str, page: u64, mut present: impl FnMut(u64, u64) -> Vec<bool>) -> Vec<Range> {
    let max_pages = MAX_RANGE_BYTES / page;
    // Files in the order the shell first mapped them, each with the file pages it had.
    let mut files: Vec<(String, Vec<u64>)> = Vec::new();
    for line in maps.lines() {
        let mut fields = line.splitn(6, ' ');
        let (Some(span), _, Some(offset), _, _, Some(path)) =
            (fields.next(), fields.next(), fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let path = path.trim_start();
        if !path.starts_with('/') || path.ends_with(" (deleted)") {
            continue;
        }
        let Some((start, end)) = span.split_once('-') else { continue };
        let (Ok(start), Ok(end), Ok(offset)) =
            (u64::from_str_radix(start, 16), u64::from_str_radix(end, 16), u64::from_str_radix(offset, 16))
        else {
            continue;
        };
        let count = (end.saturating_sub(start) / page).min(max_pages);
        let first_page = offset / page;
        let pages: Vec<u64> = present(start, count)
            .iter()
            .enumerate()
            .filter(|(_, here)| **here)
            .map(|(i, _)| first_page + i as u64)
            .collect();
        match files.iter_mut().find(|(known, _)| known == path) {
            Some((_, known)) => known.extend(pages),
            None => files.push((path.to_string(), pages)),
        }
    }
    let mut ranges = Vec::new();
    for (path, mut pages) in files {
        pages.sort_unstable();
        pages.dedup();
        let mut run: Option<(u64, u64)> = None;
        for page_index in pages.into_iter().map(Some).chain(std::iter::once(None)) {
            match (run, page_index) {
                (Some((first, last)), Some(next)) if next == last + 1 => run = Some((first, next)),
                (Some((first, last)), next) => {
                    ranges.push(Range { path: path.clone(), offset: first * page, length: (last + 1 - first) * page });
                    run = next.map(|p| (p, p));
                }
                (None, next) => run = next.map(|p| (p, p)),
            }
        }
    }
    ranges.truncate(MAX_RANGES);
    ranges
}

// The write AGENTS.md "Predictable path writes" describes: our own temp file, created exclusively at 0600, then a rename.
fn write_list(list: &Path, shell: &str, ranges: &[Range]) {
    if !crate::persistdoc::replaceable(list, MAX_LIST_BYTES, format!("{HEADER}\n").as_bytes(), |bytes| {
        std::str::from_utf8(bytes).is_ok_and(valid_list)
    }) { return; }
    let Some(dir) = list.parent() else { return };
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let mut text = format!("{HEADER}\n{shell}");
    for range in ranges {
        text.push_str(&format!("\n{} {} {}", range.offset, range.length, range.path));
    }
    text.push('\n');
    let tmp = PathBuf::from(format!("{}.{}.tmp", list.display(), std::process::id()));
    let _ = fs::remove_file(&tmp);
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut file| std::io::Write::write_all(&mut file, text.as_bytes()))
        .and_then(|()| fs::rename(&tmp, list));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

fn valid_list(text: &str) -> bool {
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) { return false; }
    let Some(shell) = lines.next() else { return false };
    let parts: Vec<_> = shell.split(' ').collect();
    if parts.len() != 3 || parts[0] != "shell" || parts[1..].iter().any(|s| s.parse::<u64>().is_err()) {
        return false;
    }
    lines.all(|line| {
        let mut parts = line.splitn(3, ' ');
        parts.next().is_some_and(|s| s.parse::<u64>().is_ok())
            && parts.next().is_some_and(|s| s.parse::<u64>().is_ok_and(|n| n > 0 && n <= MAX_RANGE_BYTES))
            && parts.next().is_some_and(|s| s.starts_with('/'))
    })
}

#[cfg(test)]
#[path = "prefetch_tests.rs"]
mod tests;
