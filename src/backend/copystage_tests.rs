// A copied file is never under its own name before its bytes are whole, staged unnamed or under a hidden sibling.
use crate::backend::copyfile::{copy_any, test_set_precheck, Progress};
use crate::backend::copystage::{self, PREFIX};
use crate::backend::testdir::TestDir;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

// Three read chunks, so the copy reports bytes before its last write and a test can look mid-copy.
pub(super) const BODY: usize = 3 * 256 * 1024 + 17;
const EOPNOTSUPP: i32 = 95;
// A made-up filesystem magic, so a refusal this file caches never reaches the real test filesystem's entry.
const SIBLING_MAGIC: i64 = 0x51B1;

// Whether this folder's real filesystem takes O_TMPFILE, asked past the seams and the refusal memory, as a builder's /tmp may not.
fn takes_unnamed(parent: &Path) -> bool {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).mode(0o600).custom_flags(copystage::O_TMPFILE);
    match options.open(parent) {
        Ok(_) => true,
        Err(e) if copystage::NO_TMPFILE.contains(&e.raw_os_error().unwrap_or(0)) => false,
        Err(e) => panic!("O_TMPFILE probe of {}: {e}", parent.display()),
    }
}

const CACHE_MAGIC: i64 = 0xCAC4E;

pub(super) fn quiet<'a>(flag: &'a AtomicBool, sink: &'a mut dyn FnMut(u64, u64)) -> Progress<'a> {
    Progress { cancel: flag, on_bytes: sink, partial: None, tree: None, manifest: None, durability: None, for_move: false }
}

pub(super) fn body() -> Vec<u8> {
    (0..BODY).map(|n| (n % 251) as u8).collect()
}

pub(super) fn source(d: &TestDir) -> PathBuf {
    let src = d.dir("src").join("VID_big.mp4");
    std::fs::write(&src, body()).unwrap();
    src
}

pub(super) fn parts(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(PREFIX))
        .collect();
    names.sort();
    names
}

// Resets the thread's seams even when an assertion fails, so one test never leaks into the next.
pub(super) struct Seams;
impl Drop for Seams {
    fn drop(&mut self) {
        copystage::test_in_place(false);
        copystage::test_fail_publish(None);
        copystage::test_fail_discard(None);
        copystage::test_refuse_unnamed(None);
        test_set_precheck(None, None, None);
    }
}

// A destination that refuses O_TMPFILE, as vfat and exfat do, so the copy stages under a named sibling.
pub(super) fn sibling_only() -> Seams {
    copystage::test_refuse_unnamed(Some(EOPNOTSUPP));
    test_set_precheck(Some(u64::MAX), Some(1), Some(SIBLING_MAGIC));
    Seams
}

// A destination that writes in place, as FUSE does, where a tree cut short stays as the partial undo removes.
pub(super) fn in_place() -> Seams {
    copystage::test_in_place(true);
    Seams
}

// Mid-copy the name never shows: the bytes sit in an unnamed inode, or in one hidden sibling where O_TMPFILE is refused.
#[test]
fn a_copy_is_unnamed_until_its_bytes_are_whole() {
    let d = TestDir::new("stage-whole");
    let src = source(&d);
    let to = d.dir("dest");
    let dst = to.join("VID_big.mp4");
    let unnamed = takes_unnamed(&to);
    let flag = AtomicBool::new(false);
    let mut seen: Vec<(bool, usize, usize)> = Vec::new();
    let mut sink = |_: u64, _: u64| seen.push((dst.exists(), std::fs::read_dir(&to).unwrap().count(), parts(&to).len()));
    copy_any(&src, &dst, &mut quiet(&flag, &mut sink)).expect("the copy lands");
    assert!(seen.len() >= 3, "progress came mid-copy: {seen:?}");
    let staged = usize::from(!unnamed);
    assert!(seen.iter().all(|&(named, entries, parts)| !named && entries == staged && parts == staged),
        "the folder held {staged} hidden entries while the copy filled, unnamed={unnamed}: {seen:?}");
    assert_eq!(std::fs::read(&dst).unwrap(), body());
    assert!(parts(&to).is_empty(), "no sibling outlived the copy");
}

// Where O_TMPFILE is refused, one hidden sibling holds the bytes mid-copy and becomes the name at the end.
#[test]
fn a_copy_where_unnamed_files_are_refused_fills_one_hidden_sibling() {
    let _seams = sibling_only();
    let d = TestDir::new("stage-sibling");
    let src = source(&d);
    let to = d.dir("dest");
    let dst = to.join("VID_big.mp4");
    let flag = AtomicBool::new(false);
    let mut seen: Vec<(bool, usize)> = Vec::new();
    let mut sink = |_: u64, _: u64| seen.push((dst.exists(), parts(&to).len()));
    copy_any(&src, &dst, &mut quiet(&flag, &mut sink)).expect("the copy lands");
    assert!(seen.iter().all(|&(named, staged)| !named && staged == 1), "the name stayed absent while one sibling filled: {seen:?}");
    assert_eq!(std::fs::read(&dst).unwrap(), body());
    assert!(parts(&to).is_empty(), "the sibling became the name");
}

// The first refusal on a filesystem is remembered, so later files there go straight to the sibling.
#[test]
fn a_filesystem_that_refuses_unnamed_files_is_probed_once() {
    let _seams = Seams;
    let d = TestDir::new("stage-probe");
    let src = source(&d);
    let to = d.dir("dest");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    test_set_precheck(Some(u64::MAX), Some(1), Some(CACHE_MAGIC));
    copystage::test_take_unnamed_tries();
    copystage::test_refuse_unnamed(Some(EOPNOTSUPP));
    copy_any(&src, &to.join("one.mp4"), &mut quiet(&flag, &mut sink)).expect("the refusal falls back to the sibling");
    copystage::test_refuse_unnamed(None);
    copy_any(&src, &to.join("two.mp4"), &mut quiet(&flag, &mut sink)).expect("the second copy lands");
    assert_eq!(copystage::test_take_unnamed_tries(), 1, "the second file on that filesystem did not ask again");
    test_set_precheck(None, None, None);
    copy_any(&src, &to.join("three.mp4"), &mut quiet(&flag, &mut sink)).expect("the real filesystem lands");
    if takes_unnamed(&to) {
        assert_eq!(copystage::test_take_unnamed_tries(), 1, "another filesystem is probed on its own");
    } else {
        // A real refusal is remembered process-wide, so an earlier test may have spent this filesystem's probe already.
        assert!(copystage::test_take_unnamed_tries() <= 1, "the refusing test filesystem was probed at most once");
        copy_any(&src, &to.join("four.mp4"), &mut quiet(&flag, &mut sink)).expect("the refusing filesystem lands");
        assert_eq!(copystage::test_take_unnamed_tries(), 0, "the refusing test filesystem is not asked again");
        assert_eq!(std::fs::read(to.join("four.mp4")).unwrap(), body());
    }
    for name in ["one.mp4", "two.mp4", "three.mp4"] {
        assert_eq!(std::fs::read(to.join(name)).unwrap(), body());
    }
}

// The staged file keeps what the create gave it and what the copy carries after: mode, mtime and tags.
#[test]
fn a_staged_copy_keeps_its_mode_mtime_and_tags() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    for sibling in [false, true] {
        let _seams = if sibling { sibling_only() } else { Seams };
        let d = TestDir::new("stage-attrs");
        let src = source(&d);
        std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o600)).unwrap();
        let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
        std::fs::File::options().write(true).open(&src).unwrap().set_modified(old).unwrap();
        crate::backend::tags::write_names(&src, &["Red".to_string()]).unwrap();
        let dst = d.dir("dest").join("copy.bin");
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        copy_any(&src, &dst, &mut quiet(&flag, &mut sink)).expect("the copy lands");
        let meta = std::fs::metadata(&dst).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600, "sibling={sibling}");
        assert_eq!(meta.mtime(), 1_600_000_000, "sibling={sibling}");
        assert_eq!(crate::backend::tags::read(&dst).unwrap(), vec!["Red"], "the tags rode the staged inode, sibling={sibling}");
    }
}

// A cancel or a failure mid-copy leaves neither the name nor a sibling, and nothing to journal.
#[test]
fn a_cancelled_or_failed_copy_leaves_nothing() {
    for sibling in [false, true] {
        let _seams = if sibling { sibling_only() } else { Seams };
        let d = TestDir::new("stage-cancel");
        let src = source(&d);
        let to = d.dir("dest");
        let dst = to.join("VID_big.mp4");
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| flag.store(true, Ordering::Relaxed);
        let e = copy_any(&src, &dst, &mut quiet(&flag, &mut sink)).expect_err("cancelled");
        assert_eq!(e.msg, "cancelled");
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        let mut p = quiet(&flag, &mut sink);
        let e = copy_any(Path::new("/proc/self/mem"), &to.join("mem.bin"), &mut p).expect_err("offset 0 of a mem file is not mapped");
        assert_ne!(e.msg, "cancelled");
        assert!(p.partial.is_none(), "nothing is left for undo to remove, sibling={sibling}");
        assert_eq!(std::fs::read_dir(&to).unwrap().count(), 0, "sibling={sibling}");
    }
}

// A name another writer takes mid-copy wins: the copy refuses as the exclusive create did and leaves nothing.
#[test]
fn a_name_taken_mid_copy_is_kept_and_the_copy_refuses() {
    for sibling in [false, true] {
        let _seams = if sibling { sibling_only() } else { Seams };
        let d = TestDir::new("stage-conflict");
        let src = source(&d);
        let to = d.dir("dest");
        let dst = to.join("VID_big.mp4");
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| if !dst.exists() { std::fs::write(&dst, "theirs").unwrap() };
        let mut p = quiet(&flag, &mut sink);
        let e = copy_any(&src, &dst, &mut p).expect_err("the name is taken");
        assert!(e.msg.contains("exists"), "the refusal names the taken name: {}", e.msg);
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "theirs");
        assert!(parts(&to).is_empty() && p.partial.is_none(), "sibling={sibling}");
    }
}

#[path = "copypublish_tests.rs"]
mod publish;
#[path = "copyfolder_tests.rs"]
mod folder;
