// Extraction tests on synthetic RAWs: a TIFF with one embedded JPEG, and the cache beside it in a sandbox.
use super::*;
use crate::backend::rawexif as ex;
use crate::backend::aliases::Aliases;
use crate::backend::rawtiff::tests::{jpeg, pair, write, Ev, Tb};
use crate::backend::testdir::TestDir;
use crate::backend::thumbcache::{png_text, Cache};
use crate::backend::metareq::read_in;
use crate::backend::thumbs::{input_for, run_one_in, Job, Outcome, Tables};
use crate::backend::thumbspec::Thumbnailers;
use crate::backend::thumbwrite::write_marker;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::Arc;
use std::time::Duration;

// A TIFF whose IFD0 names the JPEG as its JpgFromRaw-style pair, and optionally the turn the camera stored.
fn raw_with(d: &TestDir, name: &str, jpg: &[u8], turn: Option<u32>) -> PathBuf {
    let mut t = Tb::new(true);
    let off = t.blob(jpg);
    let mut entries = pair(off, jpg.len());
    if let Some(o) = turn {
        entries.insert(0, (ex::ORIENTATION, Ev::T(3, o)));
    }
    let ifd0 = t.ifd(entries, 0);
    t.set_ifd0(ifd0);
    write(d, name, &t.b)
}

fn set_mtime(path: &Path, secs: u64) {
    let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs)).unwrap();
}

// The same JPEG with 80 KB of COM segments after SOI, so its bytes run past 64 KiB.
fn padded(base: &[u8]) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8];
    for _ in 0..2 {
        v.extend_from_slice(&[0xFF, 0xFE, 0x9C, 0x42]);
        v.extend_from_slice(&[0u8; 40000]);
    }
    v.extend_from_slice(&base[2..]);
    v
}

// An Exif JPEG that already names a turn of its own.
fn jpeg_turned(w: u16, h: u16, turn: u16) -> Vec<u8> {
    let mut seg = EXIF_ORIENT;
    seg[EXIF_ORIENT_AT..EXIF_ORIENT_AT + 2].copy_from_slice(&turn.to_le_bytes());
    let base = jpeg(w, h);
    [&base[..2], &seg[..], &base[2..]].concat()
}

#[test]
fn a_raw_extracts_once_per_mtime_and_again_when_it_changes() {
    let d = TestDir::new("rawprev-once");
    let cache = d.join("cache");
    let src = raw_with(&d, "a.cr2", &jpeg(1620, 1080), None);
    set_mtime(&src, 1_700_000_000);
    let first = ensure_in(&cache, &src).expect("a preview");
    assert_eq!((first.width, first.height, first.orientation), (1620, 1080, 1));
    assert_eq!(std::fs::read(&first.file).unwrap(), jpeg(1620, 1080));
    let before = std::fs::metadata(&first.file).unwrap();

    // The same version again walks nothing: the source is now garbage with the old mtime, and the copy is served untouched.
    let kept = src.metadata().unwrap().modified().unwrap();
    std::fs::write(&src, b"not a raw any more").unwrap();
    std::fs::OpenOptions::new().write(true).open(&src).unwrap().set_modified(kept).unwrap();
    let again = ensure_in(&cache, &src).expect("served from the copy");
    assert_eq!((again.width, again.height), (1620, 1080));
    let after = std::fs::metadata(&again.file).unwrap();
    assert_eq!((before.ino(), before.mtime_nsec()), (after.ino(), after.mtime_nsec()), "a fresh copy was rewritten");

    // A changed file extracts again.
    let src = raw_with(&d, "a.cr2", &jpeg(800, 600), None);
    set_mtime(&src, 1_700_000_100);
    let changed = ensure_in(&cache, &src).expect("the new preview");
    assert_eq!((changed.width, changed.height), (800, 600));
    assert_eq!(std::fs::read(&changed.file).unwrap(), jpeg(800, 600));
}

#[test]
fn the_copy_is_private_and_a_leftover_temp_does_not_survive() {
    let d = TestDir::new("rawprev-mode");
    let cache = d.join("cache").join("flea").join("raw");
    let got = ensure_in(&cache, &raw_with(&d, "c.cr2", &jpeg(640, 480), None)).expect("a preview");
    assert_eq!(std::fs::metadata(&got.file).unwrap().permissions().mode() & 0o777, FILE_MODE);
    assert_eq!(std::fs::metadata(&cache).unwrap().permissions().mode() & 0o777, DIR_MODE);
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);
}

#[test]
fn the_turn_is_added_only_when_the_jpeg_names_none() {
    let d = TestDir::new("rawprev-turn");
    let cache = d.join("cache");
    let plain = jpeg(1620, 1080);
    let got = ensure_in(&cache, &raw_with(&d, "p.cr2", &plain, Some(6))).expect("a preview");
    assert_eq!(got.orientation, 6);
    let bytes = std::fs::read(&got.file).unwrap();
    assert_eq!(bytes.len(), plain.len() + EXIF_ORIENT.len());
    assert_eq!((&bytes[..2], &bytes[38..]), (&plain[..2], &plain[2..]));

    // The JPEG's own turn wins over the RAW's.
    let own = jpeg_turned(1620, 1080, 8);
    let kept = ensure_in(&cache, &raw_with(&d, "o.cr2", &own, Some(6))).expect("a preview");
    assert_eq!(kept.orientation, 8);
    assert_eq!(std::fs::read(&kept.file).unwrap(), own);

    // No turn in the RAW, or the identity turn, adds nothing.
    for (name, turn) in [("n.cr2", None), ("i.cr2", Some(1))] {
        let got = ensure_in(&cache, &raw_with(&d, name, &plain, turn)).expect("a preview");
        assert_eq!((got.orientation, std::fs::read(&got.file).unwrap()), (1, plain.clone()), "{name}");
    }
}

#[test]
fn a_file_with_no_preview_reports_none_and_writes_nothing() {
    let d = TestDir::new("rawprev-none");
    let cache = d.join("cache");
    let mut t = Tb::new(true);
    let ifd0 = t.ifd(vec![(ex::ORIENTATION, Ev::T(3, 6))], 0);
    t.set_ifd0(ifd0);
    assert!(ensure_in(&cache, &write(&d, "bare.cr2", &t.b)).is_none());
    assert!(ensure_in(&cache, &d.file("note.cr2", "plain text")).is_none());
    assert!(ensure_in(&cache, &d.join("missing.cr2")).is_none());
    assert!(!cache.exists() || std::fs::read_dir(&cache).unwrap().count() == 0);
}

#[test]
fn a_jpgfromraw_past_64_kib_is_copied_whole() {
    let d = TestDir::new("rawprev-rw2");
    let pic = padded(&jpeg(1920, 1080));
    assert!(pic.len() > 65_536);
    let mut t = Tb { b: b"IIU\x00\x08\x00\x00\x00".to_vec(), le: true };
    let ifd0 = t.ifd(vec![(ex::RW2_JPGFROMRAW, Ev::B(7, pic.clone()))], 0);
    t.set_ifd0(ifd0);
    let got = ensure_in(&d.join("cache"), &write(&d, "big.rw2", &t.b)).expect("the preview");
    assert_eq!((got.width, got.height), (1920, 1080));
    assert_eq!(std::fs::read(&got.file).unwrap(), pic);
}

#[test]
fn two_threads_extracting_one_file_both_get_a_whole_copy() {
    let d = TestDir::new("rawprev-race");
    let cache = d.join("cache");
    let pic = padded(&jpeg(1000, 700));
    let src = raw_with(&d, "r.cr2", &pic, None);
    let ends: Vec<_> = (0..4)
        .map(|_| {
            let (cache, src) = (cache.clone(), src.clone());
            std::thread::spawn(move || ensure_in(&cache, &src).map(|e| (e.width, e.height)))
        })
        .collect();
    for end in ends {
        assert_eq!(end.join().unwrap(), Some((1000, 700)));
    }
    assert_eq!(std::fs::read(copy_path(&cache, &src)).unwrap(), pic);
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1, "a temp survived");
}

// A thumbnailer for image/jpeg that fails unless its input starts with the JPEG magic, and then writes the PNG it was given as printf escapes.
fn jpeg_only_thumbnailer(png: &[u8]) -> Thumbnailers {
    let octal: String = png.iter().map(|b| format!(r"\\{:03o}", b)).collect();
    let script = format!(r#"[ \"$(head -c 2 \"$0\" | od -An -tx1 | tr -d ' ')\" = ffd8 ] && printf '{octal}' > \"$1\""#);
    let body = format!("[Thumbnailer Entry]\nTryExec=/bin/sh\nExec=/bin/sh -c \"{script}\" %i %o\nMimeType=image/jpeg;\n");
    Thumbnailers::from_entries(&[("jpeg.thumbnailer".to_string(), body)], &Aliases::from_str(""))
}

fn job(path: &Path, mime: &str) -> Job {
    Job { replies: None, path: path.to_path_buf(), mtime: 1_700_000_000, trash_identity: None, mime: mime.to_string(), trace: None }
}

fn tables(d: &TestDir, png: &[u8]) -> Tables {
    let aliases = Arc::new(Aliases::from_str("image/x-panasonic-raw2 image/x-panasonic-rw2\n"));
    Tables { aliases, specs: Arc::new(jpeg_only_thumbnailer(png)), cache: Cache::at(d.join("cache").join("thumbnails")), worker: None }
}

// Where the route puts its copies in a test: inside the sandbox, never the user's cache.
fn raw_dir(d: &TestDir) -> PathBuf {
    d.join("cache").join("flea").join("raw")
}

#[test]
fn the_thumbnailer_of_a_camera_raw_is_handed_the_embedded_jpeg_and_a_jpeg_is_handed_itself() {
    let d = TestDir::new("rawprev-input");
    let png = d.join("one.png");
    write_marker(&png, "file:///x", 0, None).unwrap();
    let png = std::fs::read(&png).unwrap();
    let t = tables(&d, &png);
    let src = raw_with(&d, "t.rw2", &jpeg(1620, 1080), None);
    let plain = write(&d, "plain.jpg", &jpeg(10, 10));
    let bare = write(&d, "bare.rw2", b"II*\x00\x08\x00\x00\x00\x00\x00\x00\x00\x00\x00");
    let dir = raw_dir(&d);
    let (copy, own, none) = (
        input_for(&t, &job(&src, "image/x-panasonic-raw2"), &dir),
        input_for(&t, &job(&plain, "image/jpeg"), &dir),
        input_for(&t, &job(&bare, "image/x-panasonic-rw2"), &dir),
    );
    let copy = copy.expect("the copy");
    assert_eq!(copy, copy_path(&dir, &src));
    assert_eq!(own, Some(plain));
    assert_eq!(none, None, "a RAW with no preview has nothing to hand over");
    // The spec's own program, run unjailed on that input, writes the PNG only because the input is the JPEG.
    let spec = t.specs.for_mime("image/x-panasonic-raw2", &t.aliases).unwrap();
    let out = d.join("out.png");
    let (_, inner) = crate::backend::thumbargv::argv(spec, &copy, &out, 256).unwrap();
    assert!(std::process::Command::new(&inner[0]).args(&inner[1..]).status().unwrap().success());
    assert_eq!(std::fs::read(&out).unwrap(), png);
    let (_, inner) = crate::backend::thumbargv::argv(spec, &src, &out, 256).unwrap();
    assert!(!std::process::Command::new(&inner[0]).args(&inner[1..]).status().unwrap().success(), "the RAW itself passed the JPEG check");
}

#[test]
fn a_camera_raw_is_thumbnailed_under_its_own_key() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let d = TestDir::new("rawprev-thumb");
    let png = d.join("one.png");
    write_marker(&png, "file:///x", 0, None).unwrap();
    let t = tables(&d, &std::fs::read(&png).unwrap());
    let src = raw_with(&d, "t.rw2", &jpeg(1620, 1080), None);
    let bare = write(&d, "bare.rw2", b"II*\x00\x08\x00\x00\x00\x00\x00\x00\x00\x00\x00");
    let dir = raw_dir(&d);
    let (done, none) = (run_one_in(&t, &mut job(&src, "image/x-panasonic-raw2"), &dir), run_one_in(&t, &mut job(&bare, "image/x-panasonic-rw2"), &dir));
    let Outcome::Ready(out) = done else { panic!("the RAW was not thumbnailed") };
    assert_eq!(out, t.cache.large_path(&uri_for(&src)));
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(png_text(&bytes, "Thumb::URI"), Some(uri_for(&src)));
    assert_eq!(png_text(&bytes, "Thumb::MTime"), Some("1700000000".to_string()));
    assert!(matches!(none, Outcome::Failed));
    assert!(!d.join("cache").join("thumbnails").join("fail").exists(), "a RAW with no preview was recorded as broken");
}

// A RAW whose embedded JPEG is jpeg(w, 100), with its mtime set, which the eviction order must not follow.
fn aged(d: &TestDir, name: &str, w: u16, secs: u64) -> PathBuf {
    let src = raw_with(d, name, &jpeg(w, 100), None);
    set_mtime(&src, secs);
    src
}

// One extraction with the caps and the clock given and a counter of its own at zero, so its publish scans.
fn ensure_with(dir: &Path, src: &Path, copies: usize, bytes: u64, now: SystemTime) -> Option<Extracted> {
    let publishes = AtomicU32::new(0);
    ensure_bounded(dir, src, &Bound { copies, bytes, now, publishes: &publishes })
}

// An hour on, so every copy a test wrote is older than STALE_TEMP; the clock is a parameter because a copy's ctime cannot be set.
fn later() -> SystemTime {
    SystemTime::now() + Duration::from_secs(3600)
}

// Two files written in the same clock tick share a ctime, and the order under test is the ctime's.
fn tick() {
    std::thread::sleep(Duration::from_millis(20));
}

#[test]
fn a_copy_written_under_a_minute_ago_survives_a_pass_at_the_bound_even_for_the_oldest_raw() {
    let d = TestDir::new("rawprev-evict-young");
    let cache = d.join("cache");
    let (new, oldest) = (aged(&d, "n.cr2", 300, 3_000), aged(&d, "o.cr2", 100, 1_000));
    let first = ensure_with(&cache, &new, 1, 1, SystemTime::now()).expect("a preview");
    let second = ensure_with(&cache, &oldest, 1, 1, SystemTime::now()).expect("a preview");
    assert!(first.file.exists() && second.file.exists(), "a copy written moments ago was evicted for being over the bound");
}

#[test]
fn the_cache_holds_its_count_by_dropping_the_copy_written_longest_ago_not_the_oldest_raw() {
    let d = TestDir::new("rawprev-evict-count");
    let cache = d.join("cache");
    // Written first, newest RAW; written second, oldest RAW; written last, in between.
    let (first, second, last) = (aged(&d, "a.cr2", 100, 3_000), aged(&d, "b.cr2", 200, 1_000), aged(&d, "c.cr2", 300, 2_000));
    for src in [&first, &second] {
        ensure_with(&cache, src, usize::MAX, u64::MAX, later()).expect("a preview");
        tick();
    }
    let got = ensure_with(&cache, &last, 2, u64::MAX, later()).expect("a preview");
    assert!(got.file.exists() && copy_path(&cache, &second).exists(), "a newer copy went");
    assert!(!copy_path(&cache, &first).exists(), "the copy written longest ago stayed over the bound");
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 2);
}

#[test]
fn an_old_copy_served_from_the_cache_just_now_survives_a_scan_that_evicts_an_unused_newer_copy() {
    let d = TestDir::new("rawprev-evict-served");
    let cache = d.join("cache");
    let (a, b, c) = (aged(&d, "a.cr2", 100, 3_000), aged(&d, "b.cr2", 200, 2_000), aged(&d, "c.cr2", 300, 1_000));
    for src in [&a, &b] {
        ensure_with(&cache, src, usize::MAX, u64::MAX, later()).expect("a preview");
        tick();
    }
    // A is the older copy, and a hit serves it without a publish.
    ensure_with(&cache, &a, usize::MAX, u64::MAX, later()).expect("a preview");
    tick();
    ensure_with(&cache, &c, 2, u64::MAX, later()).expect("a preview");
    assert!(copy_path(&cache, &a).exists(), "a copy served just now was evicted first");
    assert!(!copy_path(&cache, &b).exists(), "the unused copy stayed over the bound");
}

#[test]
fn a_copy_served_under_a_minute_ago_is_spared_though_it_was_published_over_a_minute_ago() {
    let d = TestDir::new("rawprev-evict-served-young");
    let cache = d.join("cache");
    let (a, b) = (aged(&d, "a.cr2", 100, 2_000), aged(&d, "b.cr2", 200, 1_000));
    ensure_with(&cache, &a, usize::MAX, u64::MAX, SystemTime::now()).expect("a preview");
    std::thread::sleep(Duration::from_millis(500));
    ensure_with(&cache, &a, usize::MAX, u64::MAX, SystemTime::now()).expect("a preview");
    // The clock reads 59.8 s after that hit: 59.8 s on the serve and over 60 s on the publish.
    let now = SystemTime::now() + Duration::from_millis(59_800);
    ensure_with(&cache, &b, 0, 0, now).expect("a preview");
    assert!(copy_path(&cache, &a).exists(), "a copy served under a minute ago was evicted by its publish time");
}

#[test]
fn a_copy_served_between_the_scan_and_its_removal_survives_the_pass() {
    let d = TestDir::new("rawprev-evict-race");
    let cache = d.join("cache");
    let (a, b) = (aged(&d, "a.cr2", 100, 2_000), aged(&d, "b.cr2", 200, 1_000));
    for src in [&a, &b] {
        ensure_with(&cache, src, usize::MAX, u64::MAX, later()).expect("a preview");
        tick();
    }
    let publishes = AtomicU32::new(0);
    let bound = Bound { copies: 0, bytes: 0, now: later(), publishes: &publishes };
    evict_between(&cache, &bound, || mark_used(&copy_path(&cache, &a)));
    assert!(copy_path(&cache, &a).exists(), "a copy served after the scan read it was removed by the scan's old verdict");
    assert!(!copy_path(&cache, &b).exists(), "the unused copy stayed over the bound");
}

#[test]
fn a_copy_gone_before_its_removal_still_leaves_the_count() {
    let d = TestDir::new("rawprev-evict-gone");
    let cache = d.join("cache");
    let (a, b) = (aged(&d, "a.cr2", 100, 2_000), aged(&d, "b.cr2", 200, 1_000));
    for src in [&a, &b] {
        ensure_with(&cache, src, usize::MAX, u64::MAX, later()).expect("a preview");
        tick();
    }
    let publishes = AtomicU32::new(0);
    let bound = Bound { copies: 1, bytes: u64::MAX, now: later(), publishes: &publishes };
    evict_between(&cache, &bound, || std::fs::remove_file(copy_path(&cache, &a)).unwrap());
    assert!(!copy_path(&cache, &a).exists(), "the hook did not delete the copy");
    assert!(copy_path(&cache, &b).exists(), "a gone copy kept its count and evicted the next copy");
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1, "the count kept a gone copy");
}

#[test]
fn the_cache_holds_its_bytes_by_the_same_order() {
    let d = TestDir::new("rawprev-evict-bytes");
    let cache = d.join("cache");
    let (a, b) = (aged(&d, "a.cr2", 100, 2_000), aged(&d, "b.cr2", 200, 1_000));
    let first = ensure_with(&cache, &a, usize::MAX, u64::MAX, later()).expect("a preview");
    tick();
    let one = std::fs::metadata(&first.file).unwrap().len();
    let second = ensure_with(&cache, &b, usize::MAX, one, later()).expect("a preview");
    assert!(second.file.exists() && !first.file.exists(), "the copy written longest ago stayed over the byte bound");
}

#[test]
fn a_scan_runs_on_the_first_publish_and_then_once_per_32() {
    let d = TestDir::new("rawprev-evict-pace");
    let cache = d.join("cache");
    // The counter starts at 1, past the first publish's scan, so publishes 1 to 31 add copies and the 32nd scans.
    let publishes = AtomicU32::new(1);
    for n in 1..=SCAN_EVERY {
        let src = aged(&d, &format!("p{n}.cr2"), 100 + n as u16, 1_000);
        let bound = Bound { copies: 2, bytes: u64::MAX, now: later(), publishes: &publishes };
        ensure_bounded(&cache, &src, &bound).expect("a preview");
        let held = std::fs::read_dir(&cache).unwrap().count();
        if n < SCAN_EVERY {
            assert_eq!(held, n as usize, "publish {n} scanned");
        } else {
            assert_eq!(held, 2, "the 32nd publish did not scan");
        }
        tick();
    }
    // A process that has published nothing scans on its first.
    let e = TestDir::new("rawprev-evict-first");
    let fresh_cache = e.join("cache");
    ensure_with(&fresh_cache, &aged(&e, "x.cr2", 100, 1_000), 0, u64::MAX, later());
    assert_eq!(std::fs::read_dir(&fresh_cache).unwrap().count(), 0, "the first publish did not scan");
}

#[test]
fn a_stale_temp_is_swept_and_a_recent_one_and_a_foreign_file_are_left() {
    let d = TestDir::new("rawprev-evict-temp");
    let cache = d.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let temp = d.file("cache/dead.123.0.tmp", "x");
    let foreign = d.file("cache/notes.txt", "x");
    ensure_with(&cache, &aged(&d, "t.cr2", 100, 2_000), 8, u64::MAX, SystemTime::now()).expect("a preview");
    assert!(temp.exists(), "a temp written moments ago was swept");
    ensure_with(&cache, &aged(&d, "u.cr2", 200, 2_000), 8, u64::MAX, later()).expect("a preview");
    assert!(!temp.exists(), "a dead writer's temp stayed");
    assert!(foreign.exists(), "a foreign file was removed");
}

#[test]
fn a_meta_read_of_a_raw_carries_the_copy_and_leaves_the_files_own_pixels_unknown() {
    let d = TestDir::new("rawprev-meta");
    let cache = raw_dir(&d);
    let src = raw_with(&d, "m.cr2", &jpeg(1620, 1080), Some(6));
    let m = read_in(&src, false, false, None, &cache);
    let raw = m.raw.expect("the copy");
    assert_eq!((raw.width, raw.height, raw.orientation), (1620, 1080, 6));
    assert!(raw.file.starts_with(&cache), "the copy was not written under the directory the read was given");
    assert_eq!((m.width, m.height, m.orientation), (0, 0, 1), "the RAW's own pixels are unknown, the preview's are not its own");
    let plain = write(&d, "p.jpg", &jpeg(10, 10));
    let p = read_in(&plain, false, false, None, &cache);
    assert!(p.raw.is_none() && (p.width, p.height) == (10, 10));
}

#[path = "rawpreview_preserve_tests.rs"]
mod preserve;
