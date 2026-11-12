// The sample cache: three PNGs per look, named by file, mtime, colours, sizes and text, trimmed oldest first.
use crate::backend::fontinfo::render::{Ask, SAMPLES};
use crate::backend::md5;
use crate::backend::thumbcache::uri_for;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const CACHE_LEAF: &str = "flea/fonts";
// 300 looks of three files; past it the oldest go until 200 looks remain.
const CACHE_MAX_FILES: usize = 900;
const CACHE_KEEP_FILES: usize = 600;
// A work folder older than this belongs to a backend that was killed mid-job.
const STALE_WORK: Duration = Duration::from_secs(600);
pub(super) const WORK_TAG: &str = "font";
// A look's file name is the key's md5 hex, a dash, the sample index and this suffix.
const KEY_HEX_LEN: usize = 32;
const LOOK_SUFFIX: &str = ".png";
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

pub(super) fn root() -> PathBuf {
    crate::userfile::cache_home().join(CACHE_LEAF)
}

// Samples drawn before font paths were literal are keyed by the bare stamp, so this prefix makes them misses.
const KEY_VERSION: &str = "literal-imagemagick-paths-v1\n";

// text is the sample drawn, which a font with no Latin letters picks for itself.
pub(super) fn key(font: &Path, meta: &std::fs::Metadata, ask: &Ask, text: &str) -> String {
    md5::hex(format!("{}{}", KEY_VERSION, stamp(font, meta, ask, text)).as_bytes())
}

pub(super) fn stamp(font: &Path, meta: &std::fs::Metadata, ask: &Ask, text: &str) -> String {
    let sizes: Vec<String> = ask.sizes.iter().map(|s| s.to_string()).collect();
    format!(
        "{}\n{}.{}\n{}\n{}\n{}\n{}",
        uri_for(font),
        meta.mtime(),
        meta.mtime_nsec(),
        ask.fg,
        ask.bg,
        sizes.join(","),
        text
    )
}

pub(super) fn entry(root: &Path, key: &str, i: usize) -> PathBuf {
    root.join(format!("{}-{}{}", key, i, LOOK_SUFFIX))
}

pub(super) fn is_png(path: &Path) -> bool {
    use std::io::Read;
    let Ok(meta) = path.symlink_metadata() else {
        return false;
    };
    let mut head = [0u8; 8];
    meta.is_file()
        && std::fs::File::open(path)
            .and_then(|mut f| f.read_exact(&mut head))
            .is_ok()
        && head == PNG_SIGNATURE
}

// A hit needs all three, each a plain file: a planted symlink in the cache is a miss, never read.
pub(super) fn cached(root: &Path, key: &str) -> Option<Vec<PathBuf>> {
    let files: Vec<PathBuf> = (0..SAMPLES).map(|i| entry(root, key, i)).collect();
    files.iter().all(|f| is_png(f)).then_some(files)
}

// Sample input: b"0123456789abcdef0123456789abcdef-2.png", the key's hex digits, a dash, an index below SAMPLES, then the suffix.
fn is_look_name(name: &[u8]) -> bool {
    let Some(stem) = name.strip_suffix(LOOK_SUFFIX.as_bytes()) else {
        return false;
    };
    stem.len() == KEY_HEX_LEN + 2
        && stem[..KEY_HEX_LEN].iter().all(u8::is_ascii_hexdigit)
        && stem[KEY_HEX_LEN] == b'-'
        && stem[KEY_HEX_LEN + 1].is_ascii_digit()
        && usize::from(stem[KEY_HEX_LEN + 1] - b'0') < SAMPLES
}

// Oldest entries first, and only names this module writes; a killed run's work folder goes once it is stale.
pub(super) fn prune(root: &Path) {
    let Ok(dir) = std::fs::read_dir(root) else {
        return;
    };
    let mut looks: Vec<(SystemTime, PathBuf)> = Vec::new();
    for item in dir.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        let Ok(meta) = item.metadata() else { continue };
        let Ok(when) = meta.modified() else { continue };
        if is_look_name(item.file_name().as_bytes())
            && meta.is_file()
            && crate::cachewrite::allowed(&item.path())
        {
            looks.push((when, item.path()));
        } else if name.starts_with(&format!(
            "{}{}-",
            crate::backend::archivework::WORK_PREFIX,
            WORK_TAG
        )) && meta.is_dir()
            && when.elapsed().is_ok_and(|age| age > STALE_WORK)
        {
            let _ = std::fs::remove_dir_all(item.path());
        }
    }
    if looks.len() > CACHE_MAX_FILES {
        looks.sort();
        for (_, path) in &looks[..looks.len() - CACHE_KEEP_FILES] {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
