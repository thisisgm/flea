// One bounded index supplies extraction totals, the password gate and the empty-output check.
use super::archive::Formats;
use super::archivespec::row_of;
use super::{archivepass, jail, sandbox};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Component, Path};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const INDEX_BYTES: u64 = 8 * 1024 * 1024;
const INDEX_ENTRIES: usize = 65_536;

#[derive(Default)]
pub(crate) struct Index {
    pub known: bool,
    pub encrypted: Option<bool>,
    pub total: usize,
    pub bytes: u64,
    pub members: HashMap<String, u64>,
}

impl Index {
    fn unknown(encrypted: bool) -> Self {
        Self {
            encrypted: encrypted.then_some(true),
            ..Self::default()
        }
    }

    fn member(&mut self, name: &str, size: u64) -> Option<()> {
        let name = relative(name)?;
        if name.is_empty() {
            return Some(());
        }
        self.total = self.total.checked_add(1)?;
        self.bytes = self.bytes.checked_add(size)?;
        if self.total > INDEX_ENTRIES || self.members.insert(name, size).is_some() {
            return None;
        }
        Some(())
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
    let reader = std::thread::spawn(move || read_output(output, seven, &observed, INDEX_BYTES));
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
                let bytes = reader.join().ok().flatten();
                if seven && status.code() == Some(255) && password.is_none() {
                    return Index::unknown(true);
                }
                let Some(bytes) = bytes.filter(|_| status.success()) else {
                    return Index::unknown(encrypted.load(Ordering::Relaxed));
                };
                return parse(&bytes, spec.as_ref());
            }
            Ok(_) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                jail::kill_tree(&mut jailed);
                return Index::unknown(encrypted.load(Ordering::Relaxed));
            }
        }
    }
}

// Encryption is independent of member validation and survives an incomplete bounded read.
#[derive(Default)]
struct Encryption {
    body: bool,
    confirmed: bool,
}

impl Encryption {
    fn line(&mut self, bytes: &[u8]) {
        let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
        let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
        self.body |= bytes == b"----------";
        self.confirmed |= self.body && bytes == b"Encrypted = +";
    }
}

fn read_output(
    output: impl Read,
    seven: bool,
    encrypted: &AtomicBool,
    limit: u64,
) -> Option<Vec<u8>> {
    let mut reader = BufReader::new(output.take(limit + 1));
    let mut bytes = Vec::new();
    let mut encryption = Encryption::default();
    loop {
        let start = bytes.len();
        if reader.read_until(b'\n', &mut bytes).ok()? == 0 {
            return Some(bytes);
        }
        if bytes.len() as u64 > limit {
            return None;
        }
        if seven {
            encryption.line(&bytes[start..]);
            if encryption.confirmed {
                encrypted.store(true, Ordering::Relaxed);
            }
        }
    }
}

fn parse(bytes: &[u8], spec: Option<&super::archivespec::ListSpec>) -> Index {
    let mut encryption = Encryption::default();
    if spec.is_none() {
        for line in bytes.split(|byte| *byte == b'\n') {
            encryption.line(line);
        }
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Index::unknown(encryption.confirmed);
    };
    let mut index = Index {
        encrypted: Some(false),
        ..Index::default()
    };
    let (mut body, mut name, mut size) = (false, None, 0);
    for line in text.lines() {
        if let Some(spec) = spec {
            if line.trim().is_empty() {
                continue;
            }
            let Some(row) = row_of(line, spec) else {
                return Index::unknown(encryption.confirmed);
            };
            let Some(size) = line
                .split_whitespace()
                .nth(spec.size_column)
                .and_then(|s| s.parse().ok())
            else {
                return Index::unknown(encryption.confirmed);
            };
            if index.member(row.name, size).is_none() {
                return Index::unknown(encryption.confirmed);
            }
        } else {
            if line == "----------" {
                body = true;
            }
            if !body {
                continue;
            }
            if let Some(value) = line.strip_prefix("Path = ") {
                if let Some(previous) = name.take() {
                    if index.member(previous, size).is_none() {
                        return Index::unknown(encryption.confirmed);
                    }
                }
                name = Some(value);
                size = 0;
            } else if let Some(value) = line.strip_prefix("Size = ") {
                let Ok(value) = value.parse() else {
                    return Index::unknown(encryption.confirmed);
                };
                size = value;
            } else if line == "Encrypted = +" {
                index.encrypted = Some(true);
            }
        }
    }
    if let Some(name) = name {
        if index.member(name, size).is_none() {
            return Index::unknown(encryption.confirmed);
        }
    }
    index.known = spec.is_some() || body;
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_index_technical_headers_do_not_count_as_members() {
        let index = parse(b"Path = a.7z\nSize = 999\n----------\nPath = a\nSize = 17\nEncrypted = +\n\nPath = sub\nSize = 0\n", None);
        assert!(index.known);
        assert_eq!((index.total, index.bytes), (2, 17));
        assert_eq!(index.encrypted, Some(true));
    }

    #[test]
    fn extraction_index_refuses_unsafe_duplicate_and_overflowed_totals() {
        for text in [
            "Path = ../a\nSize = 1",
            "Path = /a\nSize = 1",
            "Path = a\nSize = 1\nPath = a\nSize = 2",
            "Path = a\nSize = 18446744073709551615\nPath = b\nSize = 1",
        ] {
            assert!(!parse(format!("----------\n{text}\n").as_bytes(), None).known);
        }
        assert_eq!(relative("././"), Some(String::new()));
    }

    #[test]
    fn encryption_survives_rejected_members_and_invalid_utf8() {
        for text in [
            "Path = a\nSize = 1\nPath = a\nSize = 2",
            "Path = ../a\nSize = 1",
            "Path = a\nSize = nope",
            "Path = a\nSize = 18446744073709551615\nPath = b\nSize = 1",
        ] {
            let bytes = format!("----------\n{text}\nEncrypted = +\n").into_bytes();
            let index = parse(&bytes, None);
            assert!(!index.known, "{text}");
            assert_eq!(index.encrypted, Some(true), "{text}");
            assert_eq!((index.total, index.bytes, index.members.len()), (0, 0, 0));
        }
        let index = parse(b"----------\nPath = \xff\nEncrypted = +\n", None);
        assert!(!index.known);
        assert_eq!(index.encrypted, Some(true));
    }

    #[test]
    fn bounded_output_keeps_confirmed_encryption_before_exhausting_its_budget() {
        let mut bytes = b"----------\nPath = a\nEncrypted = +\n".to_vec();
        bytes.extend_from_slice(&[b'x'; 1024]);
        let encrypted = AtomicBool::new(false);
        assert!(read_output(bytes.as_slice(), true, &encrypted, 128).is_none());
        assert_eq!(
            Index::unknown(encrypted.load(Ordering::Relaxed)).encrypted,
            Some(true)
        );
        let encrypted = AtomicBool::new(false);
        read_output(
            b"Encrypted = +\n----------\n".as_slice(),
            true,
            &encrypted,
            128,
        )
        .unwrap();
        assert!(!encrypted.load(Ordering::Relaxed));
    }
}
