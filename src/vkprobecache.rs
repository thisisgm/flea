// A fingerprinted cache in front of the launcher's Vulkan probe, so every launch but the first after a change skips it.
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
// The key lives in its own file, so this one holds only the cache.
#[path = "vkprobekey.rs"]
mod key;
// The one file layout this reads, so a future layout is a miss and not a misread.
const VERSION: &str = "flea-vulkan-probe 1";
// A cache entry is one probe answer, so anything larger is not ours.
const MAX_FILE_BYTES: u64 = 4096;
// A new cache file is owner-only, the same mode userfile gives a first-created file.
const CACHE_MODE: u32 = 0o600;
// The test hook that turns the cache off, so every launch probes and nothing is written.
const CACHE_ENV: &str = "FLEA_VK_PROBE_CACHE";
const OFF: &str = "off";
// The device list vulkan::usable returns, as vendor, device and type, in probe order.
type Devices = Vec<(u32, u32, u32)>;
// The probe answer, the device list or the sentence it refused with.
type ProbeResult = Result<Devices, String>;
// The probe through one cached function, so gui.rs stays line-neutral.
pub fn usable() -> ProbeResult {
    let off = std::env::var_os(CACHE_ENV).as_deref() == Some(OsStr::new(OFF));
    let key = key::real_key();
    let path = cache_path();
    get(key.as_deref(), path.as_deref(), off, &crate::vulkan::usable)
}
// $XDG_CACHE_HOME/flea/vulkan-probe, or None when there is no home to put it in.
fn cache_path() -> Option<PathBuf> {
    let base = match crate::userfile::env_dir("XDG_CACHE_HOME") {
        Some(dir) => dir,
        None => crate::userfile::env_dir("HOME")?.join(".cache"),
    };
    Some(base.join("flea/vulkan-probe"))
}
// The file this key was stored under, rendered for that key.
fn render(key: &str, devices: &[(u32, u32, u32)]) -> String {
    let mut text = format!("{VERSION}\n{key}\n");
    for (v, d, t) in devices {
        text.push_str(&format!("{v:04x} {d:04x} {t:x}\n"));
    }
    text
}
// Sample input: "flea-vulkan-probe 1", then the key digest, then one "8086 a788 1" line (vendor, device, type in hex).
// The stored devices for this key, or None for any other shape, which is a miss.
fn parse(text: &str, key: &str) -> Option<Devices> {
    let mut lines = text.lines();
    if lines.next()? != VERSION {
        return None;
    }
    if lines.next()? != key {
        return None;
    }
    let rest: Vec<&str> = lines.collect();
    if rest.is_empty() {
        return None;
    }
    let mut devices = Vec::new();
    for line in rest {
        let mut f = line.split_whitespace();
        let (v, d, t) = (f.next()?, f.next()?, f.next()?);
        if f.next().is_some() {
            return None;
        }
        devices.push((
            u32::from_str_radix(v, 16).ok()?,
            u32::from_str_radix(d, 16).ok()?,
            u32::from_str_radix(t, 16).ok()?,
        ));
    }
    Some(devices)
}
// The stored devices, or None when the file is missing, oversized or any other shape.
fn read_cached(path: &Path, key: &str) -> Option<Devices> {
    // Opened non-blocking through the regular-file guard, so a fifo swapped in at this path never holds the launch.
    let file = crate::backend::regfile::open_regular(path)?;
    if file.metadata().ok()?.len() > MAX_FILE_BYTES {
        return None;
    }
    let mut body = String::new();
    file.take(MAX_FILE_BYTES + 1).read_to_string(&mut body).ok()?;
    if body.len() as u64 > MAX_FILE_BYTES {
        return None;
    }
    parse(&body, key)
}
// The answer by exclusive temp plus rename, unsynced since a torn file is a miss, silent when it cannot be written.
fn write_cached(path: &Path, text: &str) {
    if !crate::persistdoc::replaceable(path, MAX_FILE_BYTES, format!("{VERSION}\n").as_bytes(), |bytes| {
        let Ok(body) = std::str::from_utf8(bytes) else { return false };
        body.lines().nth(1).is_some_and(|key| parse(body, key).is_some())
    }) { return; }
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    // The pid keeps two launchers off each other's temp, and O_EXCL never follows a planted link.
    let tmp = parent.join(format!(".vulkan-probe.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let opened = std::fs::OpenOptions::new().write(true).create_new(true).mode(CACHE_MODE).open(&tmp);
    let written = opened.and_then(|mut f| f.write_all(text.as_bytes()));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}
// A hit returns the stored devices; a miss probes and stores only an Ok, so a transient Err never pins OpenGL.
pub(crate) fn get(
    key: Option<&str>,
    path: Option<&Path>,
    off: bool,
    probe: &dyn Fn() -> ProbeResult,
) -> ProbeResult {
    let slot = match (key, path) {
        (Some(key), Some(path)) if !off => Some((key, path)),
        _ => None,
    };
    let Some((key, path)) = slot else {
        return probe();
    };
    if let Some(devices) = read_cached(path, key) {
        return Ok(devices);
    }
    let answer = probe();
    if let Ok(devices) = &answer {
        write_cached(path, &render(key, devices));
    }
    answer
}
#[cfg(test)]
#[path = "vkprobecache_tests.rs"]
mod tests;
