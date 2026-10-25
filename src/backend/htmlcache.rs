use super::{htmldeps, imagesize, md5, regfile, thumbcache};
use crate::oflags::O_NOFOLLOW;
use std::fs::{FileTimes, Metadata};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

// The companion preview cache uses the same limits as the cached camera previews.
const MAX_FILES: usize = 4096;
const MAX_BYTES: u64 = 512 * 1_048_576;
const SCAN_EVERY: u32 = 32;
const READER_GRACE: Duration = Duration::from_secs(60);
static PUBLISHED: AtomicU32 = AtomicU32::new(0);
static CACHE_LOCK: Mutex<()> = Mutex::new(());
pub(super) const MAX_WIDTH: usize = 4096;
pub(super) const MAX_HEIGHT: u32 = 8000;
pub(super) const DEFAULT_HEIGHT: usize = 600;

#[cfg(test)]
thread_local! {
    static AFTER_RECHECK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

pub fn root() -> PathBuf {
    thumbcache::default_root().join("flea-html")
}

// Sample key: path /a/index.html, 120 bytes, mtime 9 s and 20 ns, width 984; each dimension invalidates independently.
pub fn key(path: &Path, meta: &Metadata, width: usize, height: usize) -> String {
    md5::hex(format!("html-v3\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}", thumbcache::uri_for(path), meta.len(), meta.mtime(), meta.mtime_nsec(), meta.ino(), meta.dev(), width, height).as_bytes())
}

pub fn hit(file: &Path, width: usize, height: usize, folder: &Path) -> bool {
    let _guard = CACHE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    htmldeps::unchanged(file, folder) && valid_png(file, width, height)
}

pub fn valid_png(file: &Path, width: usize, height: usize) -> bool {
    if width == 0 || width > MAX_WIDTH || height == 0 || height > MAX_HEIGHT as usize { return false }
    let Ok(mut opened) = regfile::open_if_regular(file, O_NOFOLLOW) else { return false };
    let mut header = [0; 24];
    if opened.read_exact(&mut header).is_err() || !header.starts_with(b"\x89PNG\r\n\x1a\n") { return false }
    let Some((w, h)) = imagesize::from_header(&header) else { return false };
    if w as usize != width || h < height as u32 || h > MAX_HEIGHT {
        return false;
    }
    opened.set_times(FileTimes::new().set_accessed(SystemTime::now())).is_ok()
}

// A render may land only where both the page and its dependency record are this cache's own.
pub fn replaceable(dest: &Path) -> bool {
    crate::cachewrite::allowed(dest) && htmldeps::replaceable(dest)
}

pub fn publish(png: &Path, dependencies: &Path, dest: &Path) -> std::io::Result<()> {
    let _guard = CACHE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let record = htmldeps::sidecar(dest);
    crate::cachewrite::require(dest)?;
    if !replaceable(dest) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "foreign or newer-schema HTML dependencies; cache was left unchanged"));
    }
    match std::fs::remove_file(&record) {
        Ok(()) => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
        Err(error) => return Err(error),
    }
    std::fs::rename(png, dest)?;
    std::fs::rename(dependencies, record)
}

fn used(meta: &Metadata) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::new(meta.ctime().max(0) as u64, meta.ctime_nsec().max(0) as u32)
}

pub fn evict(dir: &Path) {
    if PUBLISHED.fetch_add(1, Ordering::Relaxed) % SCAN_EVERY == 0 {
        trim(dir, MAX_FILES, MAX_BYTES, SystemTime::now());
    }
}

// Recently served images survive the scan so a reply can reach its asynchronous Image reader.
fn trim(dir: &Path, files: usize, bytes: u64, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut old = Vec::new();
    let (mut count, mut total) = (0, 0u64);
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = path.symlink_metadata() else { continue };
        if meta.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let pid = name.strip_prefix(".flea-work-html-").and_then(|tail| tail.split('-').next()).and_then(|n| n.parse::<u32>().ok());
            if pid.is_some_and(|pid| !Path::new(&format!("/proc/{pid}")).exists())
                && now.duration_since(used(&meta)).is_ok_and(|age| age > READER_GRACE) {
                let _ = std::fs::remove_dir_all(&path);
            }
            continue;
        }
        if !meta.is_file() || path.extension().map_or(true, |ext| ext != "png") {
            continue;
        }
        if !htmldeps::replaceable(&path) || !crate::cachewrite::allowed(&path) { continue; }
        count += 1;
        total = total.saturating_add(meta.len());
        if now.duration_since(used(&meta)).is_ok_and(|age| age > READER_GRACE) {
            old.push((used(&meta), path, meta.len()));
        }
    }
    old.sort();
    for (seen, path, size) in old {
        if count <= files && total <= bytes { break }
        let _guard = CACHE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok(meta) = path.symlink_metadata() else { continue };
        #[cfg(test)]
        AFTER_RECHECK.with(|hook| { if let Some(run) = hook.borrow_mut().take() { run(); } });
        if meta.is_file() && used(&meta) <= seen && std::fs::remove_file(&path).is_ok() {
            let _ = std::fs::remove_file(htmldeps::sidecar(&path));
            count -= 1;
            total = total.saturating_sub(size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;


    #[test]
    fn a_render_lands_only_over_this_caches_own_page() {
        let d = TestDir::new("html-png-replaceable");
        let dest = d.join("page.png");
        assert!(replaceable(&dest), "an absent page is a miss");
        std::fs::write(&dest, b"\x89PNG\r\n\x1a\nold").unwrap();
        assert!(replaceable(&dest), "an older render is replaced");
        std::fs::write(&dest, "{\"foreign\":true}").unwrap();
        assert!(!replaceable(&dest), "another document is never rendered over");
    }

    #[test]
    fn foreign_and_newer_images_survive_read_and_first_publish() {
        let d = TestDir::new("html-png-preserve");
        let dest = d.join("existing.png");
        let png = d.join("new.png");
        let deps = d.join("new.deps");
        for original in ["{\"foreign\":true}", "{\"schemaVersion\":999}", "unparsable"] {
            std::fs::write(&dest, original).unwrap();
            std::fs::write(&png, b"\x89PNG\r\n\x1a\n").unwrap();
            std::fs::write(&deps, "{\"version\":1,\"dependencies\":[]}").unwrap();
            assert!(!hit(&dest, 300, DEFAULT_HEIGHT, d.path()));
            assert_eq!(std::fs::read_to_string(&dest).unwrap(), original);
            assert!(publish(&png, &deps, &dest).unwrap_err().to_string().contains("left unchanged"));
            trim(d.path(), 0, 0, SystemTime::now() + Duration::from_secs(120));
            assert_eq!(std::fs::read_to_string(&dest).unwrap(), original);
        }
    }

    #[test]
    fn cache_key_includes_path_size_nanosecond_mtime_and_width() {
        let d = TestDir::new("html-cache-key");
        let a = d.file("a.html", "a\n");
        let b = d.file("b.html", "b\n");
        let stamp = SystemTime::UNIX_EPOCH + Duration::new(9, 20);
        for p in [&a, &b] { std::fs::File::open(p).unwrap().set_times(FileTimes::new().set_modified(stamp)).unwrap() }
        let meta = a.metadata().unwrap();
        let first = key(&a, &meta, 984, DEFAULT_HEIGHT);
        assert_eq!(first, key(&a, &meta, 984, DEFAULT_HEIGHT));
        assert_ne!(first, key(&b, &b.metadata().unwrap(), 984, DEFAULT_HEIGHT));
        assert_ne!(first, key(&a, &meta, 300, DEFAULT_HEIGHT));
        assert_ne!(first, key(&a, &meta, 984, 625));
        std::fs::File::open(&a).unwrap().set_times(FileTimes::new().set_modified(stamp + Duration::from_nanos(1))).unwrap();
        assert_ne!(first, key(&a, &a.metadata().unwrap(), 984, DEFAULT_HEIGHT));
        std::fs::write(&a, "longer\n").unwrap();
        std::fs::File::open(&a).unwrap().set_times(FileTimes::new().set_modified(stamp)).unwrap();
        assert_ne!(first, key(&a, &a.metadata().unwrap(), 984, DEFAULT_HEIGHT));
    }

    #[test]
    fn eviction_obeys_both_limits_and_leaves_recent_readers() {
        let d = TestDir::new("html-cache-limit");
        for n in 0..4 { std::fs::write(d.join(&format!("{n}.png")), b"\x89PNG\r\n\x1a\nxx").unwrap(); }
        d.file("other.txt", "keep");
        let future = SystemTime::now() + Duration::from_secs(120);
        trim(d.path(), 2, 15, future);
        let pngs = std::fs::read_dir(d.path()).unwrap().flatten().filter(|e| e.path().extension().is_some_and(|x| x == "png")).count();
        assert_eq!(pngs, 1);
        assert!(d.join("other.txt").exists());
        d.file("recent.png", "ten bytes!");
        trim(d.path(), 0, 0, SystemTime::now());
        assert!(d.join("recent.png").exists());
    }

    #[test]
    fn trim_cannot_unlink_an_entry_served_between_its_recheck_and_unlink() {
        let d = TestDir::new("html-cache-race");
        let png = d.join("race.png");
        let mut header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        header.extend_from_slice(&300u32.to_be_bytes());
        header.extend_from_slice(&600u32.to_be_bytes());
        std::fs::write(&png, header).unwrap();
        std::fs::write(htmldeps::sidecar(&png), b"{\"version\":1,\"dependencies\":[]}").unwrap();
        assert!(hit(&png, 300, DEFAULT_HEIGHT, d.path()));
        trim(d.path(), 0, 0, SystemTime::now());
        assert!(png.exists(), "a served entry keeps its reader grace");
        let answer = std::sync::Arc::new(Mutex::new(None));
        let held_answer = answer.clone();
        let requested = png.clone();
        AFTER_RECHECK.with(|hook| *hook.borrow_mut() = Some(Box::new(move || {
            let locked = CACHE_LOCK.try_lock().is_err();
            let folder = requested.parent().unwrap().to_path_buf();
            let (started, running) = std::sync::mpsc::sync_channel(0);
            let reader = std::thread::spawn(move || {
                started.send(()).unwrap();
                hit(&requested, 300, DEFAULT_HEIGHT, &folder)
            });
            running.recv().unwrap();
            if locked {
                *held_answer.lock().unwrap() = Some(Err(reader));
            } else {
                *held_answer.lock().unwrap() = Some(Ok(reader.join().unwrap()));
            }
        })));
        trim(d.path(), 0, 0, SystemTime::now() + Duration::from_secs(120));
        let served = match answer.lock().unwrap().take().unwrap() {
            Ok(served) => served,
            Err(reader) => reader.join().unwrap(),
        };
        assert!(!served || png.exists(), "an entry returned to an Image reader must survive trim");
    }
}
