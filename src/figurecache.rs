// The per-user figure bytecode cache under XDG_CACHE_HOME/flea/figures (AGENTS.md "Markdown figures").
use std::fs;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use crate::oflags::{O_DIRECTORY, O_NOFOLLOW};

// The first manifest line, so a future layout is a different file and not a misread one.
const FORMAT: &str = "flea-figures 1";
pub const MANIFEST: &str = "manifest";
// The bytecode blobs one cache directory holds, named as vendor/figure-bytecode.mjs writes them.
pub const BLOBS: [&str; 2] = ["math.bc", "mermaid.bc"];
// The sources one key covers, relative to the UI tree: the bundles, the helper and the code that compiles them.
pub(crate) const KEYED_SOURCES: [&str; 6] = [
    "vendor/math.mjs",
    "vendor/mermaid.mjs",
    "vendor/figure-helper.mjs",
    "vendor/figure-bytecode.mjs",
    "vendor/figure-compile.mjs",
    "js/FigureWorker.mjs",
];
// Where an engine's shared library can sit besides next to the binary, in the search order the loader walks.
const LIBRARY_DIRS: [&str; 3] = ["/usr/lib", "/usr/lib64", "/lib64"];
const LIBRARY_PREFIX: &str = "libqjs";
// A file past this is not one of ours, so it is refused unread.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const READ_CHUNK_BYTES: usize = 256 * 1024;
const WORD_BYTES: usize = 8;
// A key is 128 bits in hex, which is also what names its directory.
pub const KEY_HEX_CHARS: usize = 32;

// Two multiply-rotate lanes over 8-byte words, then splitmix64's finaliser: 128 bits that catch an accident, not an adversary.
const LANE_A_MUL: u64 = 0x9E37_79B9_7F4A_7C15;
const LANE_B_MUL: u64 = 0xC2B2_AE3D_27D4_EB4F;
const LANE_A_ROTATE: u32 = 29;
const LANE_B_ROTATE: u32 = 31;
const MIX_FIRST: u64 = 0xBF58_476D_1CE4_E5B9;
const MIX_SECOND: u64 = 0x94D0_49BB_1331_11EB;

fn finish(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(MIX_FIRST);
    x ^= x >> 27;
    x = x.wrapping_mul(MIX_SECOND);
    x ^ (x >> 31)
}

#[derive(Default)]
pub struct Digest {
    a: u64,
    b: u64,
    length: u64,
}

impl Digest {
    // One word into both lanes; the digest is unchanged, so every stored key stays valid.
    fn mix(&mut self, w: u64) {
        self.a = (self.a ^ w).wrapping_mul(LANE_A_MUL).rotate_left(LANE_A_ROTATE);
        self.b = self.b.wrapping_add(w).wrapping_mul(LANE_B_MUL).rotate_left(LANE_B_ROTATE) ^ self.a;
    }
    // Every call but the last must hand over a multiple of eight bytes.
    fn absorb(&mut self, chunk: &[u8]) {
        let (words, tail) = chunk.as_chunks::<WORD_BYTES>();
        for word in words {
            let w = u64::from_le_bytes(*word);
            self.mix(w);
        }
        if !tail.is_empty() {
            let mut last = [0u8; WORD_BYTES];
            last[..tail.len()].copy_from_slice(tail);
            self.mix(u64::from_le_bytes(last));
        }
        self.length += chunk.len() as u64;
    }

    pub fn hex(&self) -> String {
        format!("{:016x}{:016x}", finish(self.a ^ self.length), finish(self.b.wrapping_add(self.length)))
    }
}

pub fn digest(bytes: &[u8]) -> String {
    let mut d = Digest::default();
    d.absorb(bytes);
    d.hex()
}

// The digest and size of a regular file read without following a final symlink, or None when it is not one or is too large.
pub fn file_digest(path: &Path) -> Option<(u64, String)> {
    let mut file = fs::OpenOptions::new().read(true).custom_flags(O_NOFOLLOW).open(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    let mut digest = Digest::default();
    let mut chunk = vec![0u8; READ_CHUNK_BYTES];
    loop {
        // A short read would split a word, so each chunk is filled before it is absorbed.
        let mut filled = 0;
        while filled < chunk.len() {
            match file.read(&mut chunk[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
        digest.absorb(&chunk[..filled]);
        if filled < chunk.len() {
            return Some((digest.length, digest.hex()));
        }
    }
}

// The engine's shared libraries, by content: a symlink chain counts once.
fn library_files(qjs: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = LIBRARY_DIRS.iter().map(PathBuf::from).collect();
    if let Some(beside) = qjs.parent() {
        dirs.push(beside.to_path_buf());
        dirs.push(beside.join("../lib"));
    }
    let mut found: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with(LIBRARY_PREFIX) {
                continue;
            }
            if let Ok(real) = fs::canonicalize(entry.path()) {
                if real.is_file() && !found.contains(&real) {
                    found.push(real);
                }
            }
        }
    }
    found.sort();
    found
}

// Everything the bytecode depends on, as one key: the engine binary and its libraries, then each keyed source, by content.
pub fn key(qjs: &Path, ui: &Path) -> Option<String> {
    let mut parts: Vec<String> = vec![FORMAT.to_string()];
    let (_, engine) = file_digest(&fs::canonicalize(qjs).ok()?)?;
    parts.push(format!("engine {engine}"));
    for lib in library_files(qjs) {
        let (_, sum) = file_digest(&lib)?;
        parts.push(format!("library {sum}"));
    }
    for relative in KEYED_SOURCES {
        let (_, sum) = file_digest(&ui.join(relative))?;
        parts.push(format!("{relative} {sum}"));
    }
    Some(digest(parts.join("\n").as_bytes()))
}

// A test and diagnostic hook: "off" runs every start from source and keeps nothing, "svg" turns only the bytecode off.
pub const CACHE_ENV: &str = "FLEA_FIGURE_CACHE";

fn hook_is(values: &[&str]) -> bool {
    std::env::var_os(CACHE_ENV).is_some_and(|value| values.iter().any(|v| value == *v))
}

// $XDG_CACHE_HOME/flea/figures, or None when there is no home to put it in.
fn base() -> Option<PathBuf> {
    let cache = match crate::userfile::env_dir("XDG_CACHE_HOME") {
        Some(dir) => dir,
        None => crate::userfile::env_dir("HOME")?.join(".cache"),
    };
    Some(cache.join("flea/figures"))
}

// Where the bytecode lives, or None when the hook turns it off.
pub fn root() -> Option<PathBuf> {
    if hook_is(&["off", "svg"]) { None } else { base() }
}

// Where the SVG cache lives, or None when the hook turns every cache off.
pub fn store_root() -> Option<PathBuf> {
    if hook_is(&["off"]) { None } else { base() }
}

// A key names a directory only when it is exactly this shape, so nothing else under the root is ever treated as one of ours.
pub fn is_key(name: &str) -> bool {
    name.len() == KEY_HEX_CHARS && name.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

// One manifest line: a blob's file name, its size in bytes and its content digest.
pub type BlobEntry = (String, u64, String);

// Sample output: "flea-figures 1\nkey 00112233445566778899aabbccddeeff\nmath.bc 2894225 0123456789abcdef0123456789abcdef\n".
pub fn manifest_text(key: &str, blobs: &[BlobEntry]) -> String {
    let mut text = format!("{FORMAT}\nkey {key}\n");
    for (name, size, sum) in blobs {
        text.push_str(&format!("{name} {size} {sum}\n"));
    }
    text
}

// Sample input: "flea-figures 1\nkey 00112233445566778899aabbccddeeff\nmath.bc 12 0123456789abcdef0123456789abcdef\n"
// then "mermaid.bc 34 0123456789abcdef0123456789abcdef\n"; None for any other shape, an unknown blob or a missing one.
pub fn parse_manifest(text: &str) -> Option<(String, Vec<BlobEntry>)> {
    let mut lines = text.lines();
    if lines.next()? != FORMAT {
        return None;
    }
    let key = lines.next()?.strip_prefix("key ")?.to_string();
    let mut blobs = Vec::new();
    for line in lines {
        let mut fields = line.split(' ');
        let (name, size, sum) = (fields.next()?, fields.next()?, fields.next()?);
        if fields.next().is_some() || !BLOBS.contains(&name) {
            return None;
        }
        blobs.push((name.to_string(), size.parse().ok()?, sum.to_string()));
    }
    (blobs.len() == BLOBS.len() && BLOBS.iter().all(|b| blobs.iter().any(|(n, _, _)| n == b))).then_some((key, blobs))
}

// A manifest is a few hundred bytes, so anything larger is not ours.
const MAX_MANIFEST_BYTES: u64 = 4096;

// A generation can be rebuilt only when its manifest belongs to this reader's format.
pub(crate) fn replaceable(dir: &Path, key: &str) -> bool {
    match fs::symlink_metadata(dir) {
        Err(e) => return e.kind() == std::io::ErrorKind::NotFound,
        Ok(meta) if !meta.is_dir() => return false,
        _ => {}
    }
    let manifest = dir.join(MANIFEST);
    manifest.exists() && crate::persistdoc::replaceable(&manifest, MAX_MANIFEST_BYTES, format!("{FORMAT}\n").as_bytes(), |bytes| {
        std::str::from_utf8(bytes).ok().and_then(parse_manifest).is_some_and(|(named, _)| named == key)
    })
}

// The directory only when it holds exactly what its manifest names for this key, and it never deletes.
pub fn verified(dir: &Path, key: &str) -> Option<PathBuf> {
    // O_NOFOLLOW on the manifest guards only its last component, so the key directory is opened without following a link too.
    let _held = fs::OpenOptions::new().read(true).custom_flags(O_DIRECTORY | O_NOFOLLOW).open(dir).ok()?;
    let mut text = String::new();
    let mut manifest = fs::OpenOptions::new().read(true).custom_flags(O_NOFOLLOW).open(dir.join(MANIFEST)).ok()?;
    let meta = manifest.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    manifest.read_to_string(&mut text).ok()?;
    let (named, blobs) = parse_manifest(&text)?;
    if named != key {
        return None;
    }
    for (name, size, sum) in blobs {
        let (found_size, found_sum) = file_digest(&dir.join(&name))?;
        if found_size != size || found_sum != sum {
            return None;
        }
    }
    Some(dir.to_path_buf())
}

#[cfg(test)]
#[path = "figurecache_tests.rs"]
mod tests;
