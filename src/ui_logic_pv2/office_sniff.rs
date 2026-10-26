//! A password-protected office file told apart from a failed conversion by its own bytes, since headless LibreOffice
//! fails on both alike: an encrypted OOXML file is an OLE compound file (`CDFV2 Encrypted`), an ODF 1.4 file is one
//! `encrypted-package` entry, and an older encrypted ODF stores content.xml encrypted. Only a bounded head, the zip's
//! end and one local header are read.

/// The bytes read from the file's start.
pub const HEAD: usize = 8;
/// The bytes read from the file's end: the end record and the largest comment a zip can carry.
pub const TAIL: usize = 22 + 65_535;
/// The bytes read at content.xml's local header: the header, the name and the data's first bytes.
pub const LOCAL: usize = 30 + 256 + 16;

const CFB_MAGIC: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
const ZIP_MAGIC: [u8; 4] = *b"PK\x03\x04";
const END_MAGIC: [u8; 4] = *b"PK\x05\x06";
const ENTRY_MAGIC: [u8; 4] = *b"PK\x01\x02";
const CONTENT: &[u8] = b"content.xml";
const MIMETYPE: &[u8] = b"mimetype";
const CONTENT_TYPES: &[u8] = b"[Content_Types].xml";
const PACKAGE: &[u8] = b"encrypted-package";
const XML: &[u8] = b"<?xml";
const BOM_XML: &[u8] = b"\xef\xbb\xbf<?xml";
const STORED: u16 = 0;

fn ext(path: &str) -> String {
    path.rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// What the first read decides: locked, not locked, or read content.xml's local header at this offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Locked,
    Open,
    Local(u64),
}

/// The verdict from the file's head and its last `tail.len()` bytes, `tail_at` being where the tail starts in the file.
pub fn first(path: &str, head: &[u8], tail: &[u8], tail_at: u64) -> Verdict {
    let ext = ext(path);
    if head.starts_with(&CFB_MAGIC) {
        // OOXML is a zip; the same name as a compound file is Office's encrypted package.
        return if matches!(ext.as_str(), "docx" | "xlsx" | "pptx") {
            Verdict::Locked
        } else {
            Verdict::Open
        };
    }
    if !head.starts_with(&ZIP_MAGIC) || !matches!(ext.as_str(), "odt" | "ods" | "odp") {
        return Verdict::Open;
    }
    directory(tail, tail_at).unwrap_or(Verdict::Open)
}

// The central directory as the end record places it, inside the tail: an encrypted-package entry is a whole-package
// password, and content.xml's entry names the local header to read.
fn directory(tail: &[u8], tail_at: u64) -> Option<Verdict> {
    let mut content = None;
    for (name, at) in entries(tail, tail_at)? {
        if name == PACKAGE {
            return Some(Verdict::Locked);
        }
        if name == CONTENT {
            content = Some(u64::from(u32_at(tail, at + 42)?));
        }
    }
    content.map(Verdict::Local)
}

// Where the end record sits in the tail; a zip always has one there, since the tail holds the largest comment.
fn end_record(tail: &[u8]) -> Option<usize> {
    (0..=tail.len().checked_sub(22)?)
        .rev()
        .find(|&at| tail[at..].starts_with(&END_MAGIC))
}

// Each central-directory entry's name and offset in the tail; None when the directory cannot be read from the tail.
fn entries(tail: &[u8], tail_at: u64) -> Option<Vec<(&[u8], usize)>> {
    let end = end_record(tail)?;
    let count = u16_at(tail, end + 10)? as usize;
    let dir_at = u64::from(u32_at(tail, end + 16)?);
    let mut at = usize::try_from(dir_at.checked_sub(tail_at)?).ok()?;
    let mut list = Vec::with_capacity(count);
    for _ in 0..count {
        if !tail.get(at..)?.starts_with(&ENTRY_MAGIC) {
            return None;
        }
        let name_len = u16_at(tail, at + 28)? as usize;
        let extra = u16_at(tail, at + 30)? as usize;
        let comment = u16_at(tail, at + 32)? as usize;
        list.push((tail.get(at + 46..at + 46 + name_len)?, at));
        at += 46 + name_len + extra + comment;
    }
    Some(list)
}

/// Whether `path` names a zip container kind at all: an ODF or OOXML extension, never a legacy or RTF name.
pub fn is_container_path(path: &str) -> bool {
    matches!(
        ext(path).as_str(),
        "odt" | "ods" | "odp" | "docx" | "xlsx" | "pptx"
    )
}

/// A real container: a zip whose directory lists its kind's member; an unread directory or a non-container name stays real.
pub fn real_container(path: &str, head: &[u8], tail: &[u8], tail_at: u64) -> bool {
    let ext = ext(path);
    let wanted: &[u8] = match ext.as_str() {
        "odt" | "ods" | "odp" => MIMETYPE,
        "docx" | "xlsx" | "pptx" => {
            if head.starts_with(&CFB_MAGIC) {
                return true;
            }
            CONTENT_TYPES
        }
        _ => return true,
    };
    if !head.starts_with(&ZIP_MAGIC) || end_record(tail).is_none() {
        return false;
    }
    // map_or because is_none_or needs Rust 1.82 over the 1.77 floor.
    entries(tail, tail_at).map_or(true, |list| list.iter().any(|(name, _)| *name == wanted))
}

/// The whole container check over a reader of the file's bytes, `read(at, len)`, bounded like the sniff.
pub fn is_real_container(
    path: &str,
    size: u64,
    read: &mut dyn FnMut(u64, usize) -> Option<Vec<u8>>,
) -> bool {
    if !is_container_path(path) {
        return true;
    }
    let Some(head) = read(0, HEAD) else {
        return true;
    };
    let tail_at = size.saturating_sub(TAIL as u64);
    let tail = if head.starts_with(&ZIP_MAGIC) {
        let Some(tail) = read(tail_at, (size - tail_at) as usize) else {
            return true;
        };
        tail
    } else {
        Vec::new()
    };
    real_container(path, &head, &tail, tail_at)
}

/// The verdict from content.xml's local header: an entry stored whole that does not open with an XML declaration is
/// encrypted.
pub fn local(bytes: &[u8]) -> Verdict {
    let (Some(method), Some(name_len), Some(extra)) =
        (u16_at(bytes, 8), u16_at(bytes, 26), u16_at(bytes, 28))
    else {
        return Verdict::Open;
    };
    if !bytes.starts_with(&ZIP_MAGIC) || method != STORED {
        return Verdict::Open;
    }
    match bytes.get(30 + name_len as usize + extra as usize..) {
        Some(data) if !data.is_empty() && !data.starts_with(XML) && !data.starts_with(BOM_XML) => {
            Verdict::Locked
        }
        _ => Verdict::Open,
    }
}

/// The whole sniff over a reader of the file's bytes, `read(at, len)`; answers the verdict and the bytes it read.
pub fn sniff(
    path: &str,
    size: u64,
    read: &mut dyn FnMut(u64, usize) -> Option<Vec<u8>>,
) -> (bool, usize) {
    let head = read(0, HEAD).unwrap_or_default();
    let tail_at = size.saturating_sub(TAIL as u64);
    let mut bytes = head.len();
    let tail = if head.starts_with(&ZIP_MAGIC) {
        read(tail_at, (size - tail_at) as usize).unwrap_or_default()
    } else {
        Vec::new()
    };
    bytes += tail.len();
    match first(path, &head, &tail, tail_at) {
        Verdict::Locked => (true, bytes),
        Verdict::Open => (false, bytes),
        Verdict::Local(at) => {
            let header = read(at, LOCAL).unwrap_or_default();
            bytes += header.len();
            (local(&header) == Verdict::Locked, bytes)
        }
    }
}

#[cfg(test)]
#[path = "office_sniff_tests.rs"]
mod tests;
