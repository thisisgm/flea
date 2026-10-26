// `flea --figure-store`: the persistent SVG cache beside the bytecode, answering get, put and known lines; see AGENTS.md "Markdown figures".
use crate::figurecache;
use crate::json;
use crate::oflags::O_NOFOLLOW;
use std::fs;
use std::io::{BufRead, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

// The first line of an entry, so a future layout or engine is a different file and not a misread one (2: Pele, 3: SW-19, 4: v0.3.12 geometry).
const FORMAT: &str = "flea-figure-svg 4";
const SVG_DIR: &str = "svg";
const EXTENSION: &str = ".svg";
const SCRATCH_MARK: &str = ".tmp-";
// The scratch and entry modes both figure writers share with the bytecode build.
pub(crate) const DIR_MODE: u32 = 0o700;
pub(crate) const FILE_MODE: u32 = 0o600;
// The bound: the most entries and bytes kept, the oldest by last access going first.
const MAX_ENTRIES: usize = 512;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
// A figure over this is drawn every time rather than kept, and a file over it is not one of ours.
const MAX_ENTRY_BYTES: u64 = 4 * 1024 * 1024;
// The room a size line and its separators take beyond the entry's key, format line and svg.
const HEADER_SLACK_BYTES: u64 = 64;
// A request line is one figure's source and a theme, so anything near this is not one.
const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;
// A writer that died leaves scratch; it is swept once it is older than any live write.
pub(crate) const SCRATCH_STALE: Duration = Duration::from_secs(600);
// The source part of a name is this many hex characters of the digest.
const SOURCE_HEX_CHARS: usize = 16;

// Sample input: kind "math" and source "\\frac{a}{b}" name every entry drawn from them, whatever the theme.
fn source_sum(kind: &str, source: &str) -> String {
    figurecache::digest(format!("{kind}\0{source}").as_bytes())[..SOURCE_HEX_CHARS].to_string()
}

// Sample input: "mermaid\n#101315|#c0caf5|...|0|0\nfalse\nflowchart TD\n    A --> B" is kind, theme, display and source.
fn split_key(key: &str) -> Option<(&str, &str)> {
    let mut parts = key.splitn(4, '\n');
    let kind = parts.next()?;
    let source = parts.nth(2)?;
    Some((kind, source))
}

// Sample output: "00112233445566778899aabb-00112233445566778899aabbccddeeff.svg" without the line break.
fn entry_name(key: &str) -> Option<String> {
    let (kind, source) = split_key(key)?;
    Some(format!("{}-{}{EXTENSION}", source_sum(kind, source), figurecache::digest(key.as_bytes())))
}

fn is_hex(text: &str, length: usize) -> bool {
    text.len() == length && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_entry(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(EXTENSION) else { return false };
    let Some((source, key)) = stem.split_once('-') else { return false };
    is_hex(source, SOURCE_HEX_CHARS) && figurecache::is_key(key)
}

// Sample input: "<entry name>.tmp-4242" is scratch of one write; anything else is not ours.
fn is_scratch(name: &str) -> bool {
    let Some((stem, pid)) = name.split_once(SCRATCH_MARK) else { return false };
    is_entry(stem) && !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit())
}

// The whole file of one entry: header, then the key it was drawn for, then the svg.
fn encode(key: &str, svg: &str) -> Vec<u8> {
    let mut bytes = format!("{FORMAT}\n{} {}\n", key.len(), svg.len()).into_bytes();
    bytes.extend_from_slice(key.as_bytes());
    bytes.extend_from_slice(svg.as_bytes());
    bytes
}

// Sample input: "flea-figure-svg 4\n5 3\nhello<s/>" is key "hello" and svg "<s/>"; any other length or key is refused.
fn decode(bytes: &[u8], key: &str) -> Option<String> {
    let body = bytes.strip_prefix(FORMAT.as_bytes())?.strip_prefix(b"\n")?;
    let end = body.iter().position(|&b| b == b'\n')?;
    let sizes = std::str::from_utf8(&body[..end]).ok()?;
    let (key_len, svg_len) = sizes.split_once(' ')?;
    let (key_len, svg_len): (usize, usize) = (key_len.parse().ok()?, svg_len.parse().ok()?);
    let rest = &body[end + 1..];
    if key_len != key.len() || rest.len() != key_len.checked_add(svg_len)? || &rest[..key_len] != key.as_bytes() {
        return None;
    }
    String::from_utf8(rest[key_len..].to_vec()).ok()
}

// The svg drawn for exactly this key, or None for a missing, foreign, corrupt or linked entry; a hit counts as an access.
fn get(dir: &Path, key: &str) -> Option<String> {
    let mut file = fs::OpenOptions::new().read(true).custom_flags(O_NOFOLLOW).open(dir.join(entry_name(key)?)).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_ENTRY_BYTES + key.len() as u64 + FORMAT.len() as u64 + HEADER_SLACK_BYTES {
        return None;
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.read_to_end(&mut bytes).ok()?;
    let svg = decode(&bytes, key)?;
    drop(file.set_modified(SystemTime::now()));
    Some(svg)
}

// Whatever sits at a path of ours is removed as a plain file, never through a link.
fn remove_ours(path: &Path) {
    if fs::symlink_metadata(path).is_ok() {
        drop(fs::remove_file(path));
    }
}

// Writes the entry whole and swaps it in with one rename, then holds the bound; a figure too large to keep is skipped.
fn put(dir: &Path, key: &str, svg: &str, entries: usize, bytes: u64) -> Option<()> {
    if svg.len() as u64 > MAX_ENTRY_BYTES {
        return None;
    }
    let name = entry_name(key)?;
    fs::DirBuilder::new().recursive(true).mode(DIR_MODE).create(dir).ok()?;
    let target = dir.join(&name);
    if !crate::persistdoc::replaceable(&target, MAX_ENTRY_BYTES + key.len() as u64 + HEADER_SLACK_BYTES + FORMAT.len() as u64,
        format!("{FORMAT}\n").as_bytes(), |bytes| decode(bytes, key).is_some() || earlier_format(bytes)) { return None; }
    let scratch = dir.join(format!("{name}{SCRATCH_MARK}{}", std::process::id()));
    remove_ours(&scratch);
    let written = fs::OpenOptions::new().write(true).create_new(true).mode(FILE_MODE).open(&scratch).and_then(|mut file| file.write_all(&encode(key, svg)));
    if written.is_err() || fs::rename(&scratch, &target).is_err() {
        remove_ours(&scratch);
        return None;
    }
    evict(dir, entries, bytes);
    Some(())
}

// Stale scratch goes, then the least recently used entries until the count and the bytes are within the bound.
fn evict(dir: &Path, entries: usize, bytes: u64) {
    evict_leaf(dir, entries, bytes, &Names { entry: &|name| is_entry(name) && recognized(&dir.join(name)), scratch: &is_scratch, age: &|meta| meta.modified().unwrap_or(SystemTime::UNIX_EPOCH) });
}

// An entry an earlier Flea figure format wrote is still ours, so it is redrawn and evicted.
fn earlier_format(bytes: &[u8]) -> bool {
    [&b"flea-figure-svg 1\n"[..], b"flea-figure-svg 2\n", b"flea-figure-svg 3\n"].iter().any(|format| bytes.starts_with(format))
}

fn recognized(path: &Path) -> bool {
    crate::persistdoc::replaceable(path, MAX_ENTRY_BYTES + MAX_LINE_BYTES as u64, format!("{FORMAT}\n").as_bytes(), |bytes| {
        if earlier_format(bytes) { return true; }
        let Some(body) = bytes.strip_prefix(format!("{FORMAT}\n").as_bytes()) else { return false };
        let Some(end) = body.iter().position(|b| *b == b'\n') else { return false };
        let Some(key_len) = std::str::from_utf8(&body[..end]).ok().and_then(|s| s.split_once(' ')).and_then(|(n, _)| n.parse::<usize>().ok()) else { return false };
        body.get(end + 1..).and_then(|rest| rest.get(..key_len)).and_then(|key| std::str::from_utf8(key).ok())
            .is_some_and(|key| entry_name(key).as_deref() == path.file_name().and_then(|n| n.to_str()) && decode(bytes, key).is_some())
    })
}

// What a cache leaf calls its own and how old one of its files is, so another leaf can sit under the same bound.
pub(crate) struct Names<'a> {
    pub entry: &'a dyn Fn(&str) -> bool,
    pub scratch: &'a dyn Fn(&str) -> bool,
    pub age: &'a dyn Fn(&fs::Metadata) -> SystemTime,
}

pub(crate) fn evict_leaf(dir: &Path, entries: usize, bytes: u64, names: &Names) {
    let Ok(listing) = fs::read_dir(dir) else { return };
    let mut kept: Vec<(SystemTime, String, u64)> = Vec::new();
    for entry in listing.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(meta) = fs::symlink_metadata(entry.path()) else { continue };
        let stamp = (names.age)(&meta);
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let stale = SystemTime::now().duration_since(modified).is_ok_and(|age| age > SCRATCH_STALE);
        if (names.scratch)(&name) && stale {
            remove_ours(&entry.path());
        } else if (names.entry)(&name) {
            kept.push((stamp, name, meta.len()));
        }
    }
    kept.sort();
    let mut total: u64 = kept.iter().map(|(_, _, size)| size).sum();
    let mut count = kept.len();
    for (_, name, size) in kept {
        if count <= entries && total <= bytes {
            break;
        }
        remove_ours(&dir.join(name));
        count -= 1;
        total -= size;
    }
}

// True when every listed key has its own entry of this format, a plain file, so a warm helper would likely go unused; a theme, display, advance or format drawn otherwise is not known.
fn known(dir: &Path, keys: &[String]) -> bool {
    !keys.is_empty() && keys.iter().all(|key| entry_name(key).is_some_and(|name| current(&dir.join(name))))
}

// Sample input: a plain file opening "flea-figure-svg 4\n" is current; a link, a directory or an entry an earlier format wrote is not.
fn current(path: &Path) -> bool {
    let mut head = [0u8; FORMAT.len() + 1];
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file())
        && fs::OpenOptions::new().read(true).custom_flags(O_NOFOLLOW).open(path).and_then(|mut file| file.read_exact(&mut head)).is_ok()
        && head.strip_suffix(b"\n") == Some(FORMAT.as_bytes())
}

// The store directory unless a link sits there: get and put would follow it out of the cache; a swap after this check is the same-user race the 0700 root bounds.
fn safe_dir(dir: PathBuf) -> Option<PathBuf> {
    fs::symlink_metadata(&dir).map_or(true, |meta| !meta.file_type().is_symlink()).then_some(dir)
}

fn store_dir() -> Option<PathBuf> {
    figurecache::store_root().map(|root| root.join(SVG_DIR)).and_then(safe_dir)
}

// Sample input: {"op":"get","id":3,"key":"math\n#101315|...\ntrue\nx^2"}; put carries "svg" and no id, known carries "keys":["math\n#101315|...\ntrue\nx^2"].
fn answer(dir: Option<&Path>, line: &str) -> Option<String> {
    let op = json::field_str(line, "op")?;
    let id = json::field_usize(line, "id");
    let reply = |body: String| id.map(|id| format!("{{\"id\":{id},{body}}}"));
    match (op.as_str(), dir) {
        ("get", Some(dir)) => {
            let found = json::field_str(line, "key").and_then(|key| get(dir, &key));
            reply(found.map_or(String::from("\"miss\":true"), |svg| format!("\"svg\":\"{}\"", json::escape(&svg))))
        }
        ("get", None) => reply(String::from("\"miss\":true")),
        ("put", Some(dir)) => {
            if let (Some(key), Some(svg)) = (json::field_str(line, "key"), json::field_str(line, "svg")) {
                put(dir, &key, &svg, MAX_ENTRIES, MAX_BYTES);
            }
            None
        }
        ("known", _) => reply(format!("\"known\":{}", dir.is_some_and(|dir| known(dir, &json::field_str_array(line, "keys"))))),
        _ => None,
    }
}

pub fn run() -> i32 {
    let dir = store_dir();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { return 0 };
        if line.len() > MAX_LINE_BYTES {
            continue;
        }
        if let Some(reply) = answer(dir.as_deref(), &line) {
            if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
                return 0;
            }
        }
    }
    0
}

#[cfg(test)]
#[path = "figurestore_tests.rs"]
mod tests;
