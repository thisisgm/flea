use super::{model3d, regfile, thumbcache, thumbwrite};
use crate::oflags::O_NOFOLLOW;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::{Path,PathBuf};

const MAX_PNG_PIXELS: u64 = 16_777_216;
const MAX_CACHE_FILES: usize = 128;
const ORPHAN_MIN_AGE_SECS: u64 = 60;
const READER_GRACE_SECS: u64 = 60;

pub(super) fn root() -> PathBuf { thumbcache::default_root().join("flea-3d") }

pub(super) fn key(path: &Path, meta: &Metadata) -> String {
    super::md5::hex(format!("3d-v1\0{}\0{}\0{}\0{}\0{}\0{}",thumbcache::uri_for(path),meta.dev(),meta.ino(),meta.len(),meta.mtime(),meta.mtime_nsec()).as_bytes())
}

pub(super) fn same(a: &Metadata, b: &Metadata) -> bool {
    (a.dev(),a.ino(),a.len(),a.mtime(),a.mtime_nsec()) == (b.dev(),b.ino(),b.len(),b.mtime(),b.mtime_nsec())
}

pub(super) fn header(file: &Path) -> Option<(String,Vec<model3d::Thumbnail>)> {
    let (input,meta) = regfile::open_if_regular_with_meta(file,O_NOFOLLOW).ok()?;
    if meta.len() > model3d::wire::MAX_WIRE as u64 { return None }
    input.set_times(std::fs::FileTimes::new().set_accessed(std::time::SystemTime::now())).ok()?;
    model3d::wire::header(input).ok()
}

// Sample input: PNG signature, IHDR(width,height), CRC-checked IDAT chunks and an empty IEND; pixels are decoded only in the thumbnail jail.
pub(super) fn png(bytes: &[u8]) -> bool {
    if bytes.len() > model3d::MAX_THUMBNAIL || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") { return false }
    let Some((w,h)) = super::imagesize::from_header(bytes) else { return false };
    if w == 0 || h == 0 || u64::from(w)*u64::from(h) > MAX_PNG_PIXELS { return false }
    let mut at = 8usize;
    let mut image = false;
    while at+12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[at..at+4].try_into().unwrap()) as usize;
        let Some(end) = at.checked_add(12).and_then(|n| n.checked_add(len)).filter(|&n| n <= bytes.len()) else { return false };
        let kind = &bytes[at+4..at+8];
        if model3d::codecs::crc32(&bytes[at+4..end-4]) != u32::from_be_bytes(bytes[end-4..end].try_into().unwrap()) { return false }
        if at == 8 && (kind != b"IHDR" || len != 13) { return false }
        if kind == b"IDAT" { image = true }
        if kind == b"IEND" { return len == 0 && end == bytes.len() && image }
        at = end;
    }
    false
}

pub(super) fn embedded(root: &Path, key: &str, thumbnails: &[model3d::Thumbnail], plate: Option<usize>) -> Option<PathBuf> {
    let valid = |t: &&model3d::Thumbnail| match t.format.as_str() {
        "png" => png(&t.bytes), "jpg" => t.bytes.starts_with(b"\xff\xd8\xff"), "qoi" => t.bytes.starts_with(b"qoif"), _ => false,
    };
    let thumbnail = thumbnails.iter().filter(valid).find(|t| plate.is_some() && t.plate.map(|p| p as usize) == plate)
        .or_else(|| thumbnails.iter().filter(valid).find(|t| t.plate.is_none())).or_else(|| thumbnails.iter().find(valid))?;
    let name = format!("{}-{}.{}",key,thumbnail.plate.map(|n| n.to_string()).unwrap_or_else(|| "all".into()),thumbnail.format);
    let dest = root.join(name);
    if let Ok(file) = regfile::open_if_regular(&dest,O_NOFOLLOW) {
        file.set_times(std::fs::FileTimes::new().set_accessed(std::time::SystemTime::now())).ok()?;
        return Some(dest);
    }
    let temp = thumbwrite::exclusive_temp(root)?;
    if std::fs::write(&temp,&thumbnail.bytes).is_err() || std::fs::rename(&temp,&dest).is_err() { let _ = std::fs::remove_file(&temp); return None }
    Some(dest)
}

// The existing preview-cache eviction policy preserves recent readers and caps old model and image files.
pub(super) fn evict(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = path.symlink_metadata() else { continue };
        if meta.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let pid = name.strip_prefix(".flea-work-model3d-").and_then(|s| s.split('-').next()).and_then(|s| s.parse::<u32>().ok());
            if pid.is_some_and(|pid| pid > 1 && !Path::new(&format!("/proc/{pid}")).exists())
                && meta.modified().is_ok_and(|time| time.elapsed().is_ok_and(|age| age.as_secs() > ORPHAN_MIN_AGE_SECS)) {
                let _ = std::fs::remove_dir_all(path);
            }
            continue;
        }
        if !meta.is_file() { continue }
        total = total.saturating_add(meta.len());
        files.push((meta.accessed().unwrap_or(std::time::SystemTime::UNIX_EPOCH),path,meta.len()));
    }
    files.sort_unstable();
    let mut count = files.len();
    for (time,path,len) in files {
        if count <= MAX_CACHE_FILES && total <= model3d::wire::MAX_WIRE as u64 { break }
        if time.elapsed().is_ok_and(|d| d.as_secs() > READER_GRACE_SECS) && std::fs::remove_file(path).is_ok() {
            count -= 1;
            total = total.saturating_sub(len);
        }
    }
}

