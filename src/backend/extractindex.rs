// One streamed index supplies extraction totals, the password gate and the empty-output check.
use super::archive::Formats;
use super::archivespec::{row_of, ListSpec};
use super::extractrun::LINE_LIMIT;
use super::{archivepass, jail, sandbox};
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Component, Path};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// These bound the names retained for progress samples, never the totals the safety checks count.
const INDEX_BYTES: u64 = 8 * 1024 * 1024;
const INDEX_ENTRIES: usize = 65_536;
// The listing child is polled this often until it exits or the listing deadline passes.
const POLL: Duration = Duration::from_millis(10);

#[derive(Default)]
pub(crate) struct Index {
    pub known: bool,
    pub encrypted: Option<bool>,
    pub total: usize,
    pub bytes: u64,
    // The room the destination needs, where a later version of a path overwrites the earlier one, so each tracked path adds its largest.
    pub space: u64,
    pub members: HashMap<String, u64>,
}

impl Index {
    fn unknown(encrypted: bool) -> Self {
        Self {
            encrypted: encrypted.then_some(true),
            ..Self::default()
        }
    }
}

// Only relative members can be sampled; parent components and absolute names never leave staging.
pub(crate) fn relative(name: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in Path::new(name).components() {
        match part {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

pub(crate) fn read(
    formats: &Formats,
    archive: &Path,
    password: Option<&str>,
    cancel: &AtomicBool,
) -> Index {
    let seven = formats.seven_reads(archive);
    let command = if seven {
        formats
            .technical_list_argv(archive)
            .map(|argv| (argv, None))
    } else {
        formats
            .list_argv(archive)
            .map(|(argv, spec)| (argv, Some(spec)))
    };
    let Some((inner, spec)) = command else {
        return Index::default();
    };
    if !sandbox::available() || cancel.load(Ordering::Relaxed) {
        return Index::default();
    }
    let mut full = sandbox::wrap_readonly(&inner, archive);
    sandbox::add_status(&mut full, inner.len());
    let Ok(mut jailed) = jail::spawn_jailed(&full, |command| {
        command.stdin(if password.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());
    }) else {
        return Index::default();
    };
    let Some(output) = jailed.child.stdout.take() else {
        jail::kill_tree(&mut jailed);
        return Index::default();
    };
    let encrypted = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&encrypted);
    let reader = std::thread::spawn(move || read_output(output, spec.as_ref(), &observed));
    let deadline = Instant::now() + Duration::from_millis(super::archivelist::ARCHIVE_READ_MS);
    let mut prompt = match archivepass::Prompt::new(&mut jailed.child, password) {
        Ok(prompt) => prompt,
        Err(_) => {
            jail::kill_tree(&mut jailed);
            return Index::unknown(encrypted.load(Ordering::Relaxed));
        }
    };
    loop {
        if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
            jail::kill_tree(&mut jailed);
            return Index::unknown(encrypted.load(Ordering::Relaxed));
        }
        archivepass::Prompt::advance(&mut prompt);
        match jailed.child.try_wait() {
            Ok(Some(status)) if reader.is_finished() => {
                let index = reader.join().ok().flatten();
                if seven && status.code() == Some(archivepass::NEEDS_PASSWORD_CODE) && password.is_none() {
                    return Index::unknown(true);
                }
                return index
                    .filter(|_| status.success())
                    .unwrap_or_else(|| Index::unknown(encrypted.load(Ordering::Relaxed)));
            }
            Ok(_) => std::thread::sleep(POLL),
            Err(_) => {
                jail::kill_tree(&mut jailed);
                return Index::unknown(encrypted.load(Ordering::Relaxed));
            }
        }
    }
}

// Sample: the listing child's stdout, one "\n"-ended line per read.
// A line past LINE_LIMIT is judged from its first piece; the rest is read and discarded in bounded pieces.
fn read_output(output: impl Read, spec: Option<&ListSpec>, encrypted: &AtomicBool) -> Option<Index> {
    let mut reader = BufReader::new(output);
    let mut listing = Listing { spec, ..Listing::default() };
    let mut line = Vec::new();
    loop {
        let (count, cut) = bounded_line(&mut reader, &mut line).ok()?;
        if count == 0 {
            return Some(listing.finish());
        }
        listing.line(&line, cut);
        if listing.encrypted {
            encrypted.store(true, Ordering::Relaxed);
        }
        while cut && bounded_line(&mut reader, &mut line).ok()?.1 {}
    }
}

// The count read and whether LINE_LIMIT cut the line before its "\n".
fn bounded_line(reader: &mut impl BufRead, line: &mut Vec<u8>) -> std::io::Result<(usize, bool)> {
    line.clear();
    let count = reader.by_ref().take(LINE_LIMIT as u64).read_until(b'\n', line)?;
    Ok((count, count == LINE_LIMIT && line.last() != Some(&b'\n')))
}

// Every listed member counts toward the totals; a bounded set of unique UTF-8 names is retained for progress.
#[derive(Default)]
struct Listing<'a> {
    spec: Option<&'a ListSpec>,
    index: Index,
    retained: u64,
    // The largest size of a retained name listed more than once, when a later version outgrew the first.
    largest: HashMap<String, u64>,
    body: bool,
    encrypted: bool,
    pending: Option<(String, bool)>,
    size: u64,
    failed: bool,
}

impl Listing<'_> {
    // Sample: 7z -slt lines "----------", "Path = docs/a.txt", "Size = 17", "Encrypted = +".
    // Encryption is read past a rejected member, because it is independent of member validation.
    fn line(&mut self, bytes: &[u8], cut: bool) {
        let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
        let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
        let text = String::from_utf8_lossy(bytes);
        let intact = matches!(text, Cow::Borrowed(_));
        if let Some(spec) = self.spec {
            // Every listing row ends in its member's name, so a cut row hides part of a name that can never be judged safe.
            self.failed |= cut;
            if !self.failed && !text.trim().is_empty() {
                self.failed = self.row(&text, spec, intact).is_none();
            }
            return;
        }
        self.body |= text == "----------";
        if !self.body {
            return;
        }
        self.encrypted |= text == "Encrypted = +";
        if self.failed {
            return;
        }
        if let Some(name) = text.strip_prefix("Path = ") {
            if let Some((previous, whole)) = self.pending.replace((name.to_string(), intact)) {
                self.failed = self.member(&previous, self.size, whole).is_none();
            }
            // A cut name hides its tail, which can hold a parent component, so it can never be judged safe.
            self.failed |= cut;
            self.size = 0;
        } else if let Some(size) = text.strip_prefix("Size = ") {
            match size.parse() {
                Ok(size) => self.size = size,
                Err(_) => self.failed = true,
            }
        }
    }

    // Sample: bsdtar -tv "-rw-r--r--  0 gm gm 17 Oct 18 10:00 docs/a.txt".
    fn row(&mut self, line: &str, spec: &ListSpec, intact: bool) -> Option<()> {
        let row = row_of(line, spec)?;
        let size = line.split_whitespace().nth(spec.size_column)?.parse().ok()?;
        self.member(row.name, size, intact)
    }

    // The archive root produces no destination entry, and an unsafe name or an overflowed total leaves the index unknown.
    fn member(&mut self, name: &str, size: u64, intact: bool) -> Option<()> {
        let name = relative(name)?;
        if name.is_empty() {
            return Some(());
        }
        self.index.total = self.index.total.checked_add(1)?;
        self.index.bytes = self.index.bytes.checked_add(size)?;
        // corner: a repeat of a name past the retention caps is summed, which over-counts the room and never under-counts it.
        let seen = intact.then(|| self.largest.get(&name).or_else(|| self.index.members.get(&name))).flatten().copied();
        self.index.space = self.index.space.checked_add(size.saturating_sub(seen.unwrap_or(0)))?;
        let room = self.index.members.len() < INDEX_ENTRIES
            && self.retained + name.len() as u64 <= INDEX_BYTES;
        if seen.is_some_and(|seen| size > seen) {
            self.largest.insert(name, size);
        } else if seen.is_none() && intact && room {
            self.retained += name.len() as u64;
            self.index.members.insert(name, size);
        }
        Some(())
    }

    fn finish(mut self) -> Index {
        if let Some((name, intact)) = self.pending.take().filter(|_| !self.failed) {
            self.failed = self.member(&name, self.size, intact).is_none();
        }
        if self.failed {
            return Index::unknown(self.encrypted);
        }
        self.index.known = self.spec.is_some() || self.body;
        self.index.encrypted = Some(self.encrypted);
        self.index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::archivelist::parse_reader;
    use crate::backend::archivespec::{tar_spec, ListSpec};

    fn listed(bytes: &[u8], spec: Option<&ListSpec>) -> Index {
        read_output(bytes, spec, &AtomicBool::new(false)).expect("an in-memory listing reads to its end")
    }

    // A listing child killed at the deadline: its pipe fails after the lines it already wrote.
    struct Killed;

    impl Read for Killed {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("the listing child was killed"))
        }
    }

    fn tar_rows(count: usize, name: impl Fn(usize) -> String) -> Vec<u8> {
        (0..count).flat_map(|i| format!("-rw-r--r--  0 gm gm 1 Oct 18 10:00 {}\n", name(i)).into_bytes()).collect()
    }

    #[test]
    fn extraction_index_counts_every_member_past_the_retention_cap() {
        let bytes = tar_rows(80_000, |i| format!("f{i}.txt"));
        let old = parse_reader(bytes.as_slice(), &tar_spec());
        let index = listed(&bytes, Some(&tar_spec()));
        assert!(index.known);
        assert_eq!((index.total, index.bytes), (old.produced_entries, old.unpacked));
        assert_eq!((index.total, index.bytes, index.space), (80_000, 80_000, 80_000));
        assert_eq!(index.members.len(), INDEX_ENTRIES);
    }

    #[test]
    fn extraction_index_counts_a_listing_longer_than_the_retained_bytes() {
        let bytes = tar_rows(40_000, |i| format!("f{i}-{}", "a".repeat(220)));
        assert!(bytes.len() as u64 > INDEX_BYTES);
        let index = listed(&bytes, Some(&tar_spec()));
        assert!(index.known);
        assert_eq!((index.total, index.bytes), (40_000, 40_000));
        assert!(index.members.keys().map(|name| name.len() as u64).sum::<u64>() <= INDEX_BYTES);
    }

    #[test]
    fn extraction_index_counts_duplicate_members_and_retains_one() {
        let index = listed(b"-rw-r--r--  0 gm gm 3 Oct 18 10:00 a.txt\n-rw-r--r--  0 gm gm 5 Oct 18 10:00 a.txt\n", Some(&tar_spec()));
        assert!(index.known);
        assert_eq!((index.total, index.bytes, index.space), (2, 8, 5));
        assert_eq!(index.members, HashMap::from([("a.txt".to_string(), 3)]));
    }

    // Appended versions of one path overwrite each other, so the room holds each path once at its largest version.
    #[test]
    fn extraction_index_room_holds_each_path_at_its_largest_version() {
        let row = |size: u64, name: &str| format!("-rw-r--r--  0 gm gm {size} Oct 18 10:00 {name}\n");
        let index = listed([6_000_000_000; 3].map(|size| row(size, "disk.img")).concat().as_bytes(), Some(&tar_spec()));
        assert_eq!((index.known, index.total, index.bytes, index.space), (true, 3, 18_000_000_000, 6_000_000_000));
        let index = listed([row(3, "a"), row(5, "a"), row(4, "a"), row(2, "b")].concat().as_bytes(), Some(&tar_spec()));
        assert_eq!((index.total, index.bytes, index.space), (4, 14, 7));
        let index = listed(b"----------\nPath = a\nSize = 3\nPath = a\nSize = 5\n", None);
        assert_eq!((index.total, index.bytes, index.space), (2, 8, 5));
        // A name that is not UTF-8 is never matched against another, so it always adds its own size.
        let index = listed(b"-rw-r--r--  0 gm gm 3 Oct 18 10:00 caf\xe9\n-rw-r--r--  0 gm gm 5 Oct 18 10:00 caf\xef\xbf\xbd\n-rw-r--r--  0 gm gm 7 Oct 18 10:00 caf\xe9\n", Some(&tar_spec()));
        assert_eq!((index.total, index.bytes, index.space), (3, 15, 15));
        assert_eq!(index.members, HashMap::from([("caf\u{FFFD}".to_string(), 5)]));
    }

    #[test]
    fn extraction_index_counts_a_member_whose_name_is_not_utf8() {
        let index = listed(b"-rw-r--r--  0 gm gm 7 Oct 18 10:00 caf\xe9.txt\n", Some(&tar_spec()));
        assert!(index.known);
        assert_eq!((index.total, index.bytes, index.members.len()), (1, 7, 0));
    }

    #[test]
    fn extraction_index_never_judges_a_name_cut_at_its_bound() {
        let long = "a".repeat(LINE_LIMIT + 10);
        let unsafe_tail = format!("{}/../../x", "a".repeat(65_530));
        for name in [&long, &unsafe_tail] {
            let index = listed(format!("----------\nPath = {name}\nSize = 5\nPath = b\nSize = 2\nEncrypted = +\n").as_bytes(), None);
            assert_eq!((index.known, index.total, index.encrypted), (false, 0, Some(true)));
            let index = listed(format!("-rw-r--r--  0 gm gm 4 Oct 18 10:00 {name}\n-rw-r--r--  0 gm gm 3 Oct 18 10:00 c\n").as_bytes(), Some(&tar_spec()));
            assert_eq!((index.known, index.total), (false, 0));
        }
        // A cut line that names no member is discarded whole, and the next line still parses.
        let index = listed(format!("----------\nPath = b\nSize = 2\nComment = {long}\nPath = c\nSize = 3\n").as_bytes(), None);
        assert_eq!((index.known, index.total, index.bytes), (true, 2, 5));
        assert_eq!(index.members, HashMap::from([("b".to_string(), 2), ("c".to_string(), 3)]));
    }

    #[test]
    fn extraction_index_technical_headers_do_not_count_as_members() {
        let index = listed(b"Path = a.7z\nSize = 999\n----------\nPath = a\nSize = 17\nEncrypted = +\n\nPath = sub\nSize = 0\n", None);
        assert!(index.known);
        assert_eq!((index.total, index.bytes), (2, 17));
        assert_eq!(index.encrypted, Some(true));
    }

    #[test]
    fn extraction_index_refuses_unsafe_and_overflowed_totals() {
        for text in [
            "Path = ../a\nSize = 1",
            "Path = /a\nSize = 1",
            "Path = a\nSize = 18446744073709551615\nPath = b\nSize = 1",
        ] {
            assert!(!listed(format!("----------\n{text}\n").as_bytes(), None).known);
        }
        assert_eq!(relative("././"), Some(String::new()));
    }

    #[test]
    fn encryption_survives_rejected_members_and_invalid_utf8() {
        for text in [
            "Path = ../a\nSize = 1",
            "Path = a\nSize = nope",
            "Path = a\nSize = 18446744073709551615\nPath = b\nSize = 1",
        ] {
            let bytes = format!("----------\n{text}\nEncrypted = +\n").into_bytes();
            let index = listed(&bytes, None);
            assert!(!index.known, "{text}");
            assert_eq!(index.encrypted, Some(true), "{text}");
            assert_eq!((index.total, index.bytes, index.members.len()), (0, 0, 0));
        }
        let index = listed(b"----------\nPath = a\nSize = 1\nPath = a\nSize = 2\nEncrypted = +\n", None);
        assert_eq!((index.known, index.total, index.bytes, index.encrypted), (true, 2, 3, Some(true)));
        let index = listed(b"----------\nPath = \xff\nEncrypted = +\n", None);
        assert_eq!((index.known, index.total, index.members.len()), (true, 1, 0));
        assert_eq!(index.encrypted, Some(true));
    }

    #[test]
    fn a_killed_listing_keeps_the_encryption_it_already_confirmed() {
        let encrypted = AtomicBool::new(false);
        let cut = b"----------\nPath = a\nEncrypted = +\n".as_slice().chain(Killed);
        assert!(read_output(cut, None, &encrypted).is_none());
        assert_eq!(Index::unknown(encrypted.load(Ordering::Relaxed)).encrypted, Some(true));
        let encrypted = AtomicBool::new(false);
        assert!(read_output(b"Encrypted = +\n----------\n".as_slice().chain(Killed), None, &encrypted).is_none());
        assert!(!encrypted.load(Ordering::Relaxed));
    }
}
