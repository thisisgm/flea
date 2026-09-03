// The copy primitives every transfer is built from: streaming, symlink-preserving, and refusing to overwrite.
use crate::error::{from_io, FleaError};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

// Big enough that the syscall count stops mattering, small enough that a cancel is noticed promptly.
const CHUNK: usize = 256 * 1024;
// rename(2) sets EXDEV when the two paths are on different filesystems, which is the one failure that means "copy instead".
const EXDEV: i32 = 18;
// open(2) O_NOFOLLOW on linux x86-64; declared here because this tree takes no libc crate.
const O_NOFOLLOW: i32 = 0o400000;

// What a copy reports as it runs; a directory has no total without a sweep, so it reports 0 and renders indeterminate.
pub struct Progress<'a> {
    pub cancel: &'a AtomicBool,
    pub on_bytes: &'a mut dyn FnMut(u64, u64),
    // The destination a copy created and then failed to finish for a reason other than a cancel. It
    // stays on disk, because removing it would destroy data on a transient error, and the caller
    // journals it so undo removes it as one step. A cancel never sets it: the cancel path removes.
    pub partial: Option<PathBuf>,
}

pub fn cancelled(p: &Progress) -> bool {
    p.cancel.load(Ordering::Relaxed)
}

// Copies one regular file, creating the destination exclusively so an existing file is never destroyed.
pub fn copy_file(src: &Path, dst: &Path, total: u64, p: &mut Progress) -> Result<(), FleaError> {
    // O_NOFOLLOW closes a same-uid race: copy_any already sent symlinks to copy_symlink, so a src
    // that is a symlink here was swapped in after that stat, and it must not be followed.
    let mut r = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW)
        .open(src)
        .map_err(|e| from_io("copy", &src.to_string_lossy(), &e))?;
    let mut w = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)
        .map_err(|e| from_io("copy", &dst.to_string_lossy(), &e))?;
    // From here the destination exists, and every failure below leaves it for the caller to journal.
    let mut buf = vec![0u8; CHUNK];
    let mut done: u64 = 0;
    loop {
        if cancelled(p) {
            // The partial file goes with the cancel: a half-written destination is not a result anyone asked for.
            drop(w);
            let _ = std::fs::remove_file(dst);
            return Err(cancel_err(dst));
        }
        let n = match r.read(&mut buf) {
            Ok(n) => n,
            Err(e) => return Err(left_partial(p, dst, from_io("copy", &src.to_string_lossy(), &e))),
        };
        if n == 0 {
            break;
        }
        if let Err(e) = w.write_all(&buf[..n]) {
            return Err(left_partial(p, dst, from_io("copy", &dst.to_string_lossy(), &e)));
        }
        done += n as u64;
        (p.on_bytes)(done, total);
    }
    if let Err(e) = w.flush() {
        return Err(left_partial(p, dst, from_io("copy", &dst.to_string_lossy(), &e)));
    }
    Ok(())
}

// A failure after the destination was created, and not a cancel: the partial stays, and is reported for the journal.
fn left_partial(p: &mut Progress, dst: &Path, e: FleaError) -> FleaError {
    p.partial = Some(dst.to_path_buf());
    e
}

// A symlink is copied as a symlink and never followed, matching cp -a and every rival in the parity audit.
pub fn copy_symlink(src: &Path, dst: &Path) -> Result<(), FleaError> {
    let target = std::fs::read_link(src).map_err(|e| from_io("copy", &src.to_string_lossy(), &e))?;
    std::os::unix::fs::symlink(&target, dst).map_err(|e| from_io("copy", &dst.to_string_lossy(), &e))
}

// Copies a file, a symlink or a whole directory tree. The destination must not already exist.
pub fn copy_any(src: &Path, dst: &Path, p: &mut Progress) -> Result<(), FleaError> {
    let meta = src
        .symlink_metadata()
        .map_err(|e| from_io("copy", &src.to_string_lossy(), &e))?;
    if meta.file_type().is_symlink() {
        return copy_symlink(src, dst);
    }
    if meta.is_dir() {
        return copy_dir(src, dst, p);
    }
    copy_file(src, dst, meta.len(), p)
}

fn copy_dir(src: &Path, dst: &Path, p: &mut Progress) -> Result<(), FleaError> {
    std::fs::create_dir(dst).map_err(|e| from_io("copy", &dst.to_string_lossy(), &e))?;
    let r = copy_dir_entries(src, dst, p);
    if r.is_err() {
        if cancelled(p) {
            // The tree goes with the cancel, the same rule copy_file already applies to a partial file: a
            // half-copied directory is not a result anyone asked for, and no journal step records one.
            // Gated on the flag rather than the message, because a nested copy_file returns its own cancel.
            let _ = std::fs::remove_dir_all(dst);
            p.partial = None;
        } else {
            // Any other failure leaves what was copied, since removing it would destroy data on a
            // transient error, and reports the whole tree as the one partial the journal records.
            p.partial = Some(dst.to_path_buf());
        }
    }
    r
}

fn copy_dir_entries(src: &Path, dst: &Path, p: &mut Progress) -> Result<(), FleaError> {
    let entries = std::fs::read_dir(src).map_err(|e| from_io("copy", &src.to_string_lossy(), &e))?;
    for entry in entries {
        if cancelled(p) {
            return Err(cancel_err(dst));
        }
        let entry = entry.map_err(|e| from_io("copy", &src.to_string_lossy(), &e))?;
        copy_any(&entry.path(), &dst.join(entry.file_name()), p)?;
    }
    Ok(())
}

// Same filesystem is a rename; a different one is copy-then-remove, and the source only goes once the copy is complete.
pub fn move_any(src: &Path, dst: &Path, p: &mut Progress) -> Result<(), FleaError> {
    match crate::backend::ops::rename_noreplace(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if e.msg.contains("os error 18") || is_exdev(&e) => {
            copy_any(src, dst, p)?;
            remove_any(src)
        }
        Err(e) => Err(e),
    }
}

fn is_exdev(e: &FleaError) -> bool {
    e.msg.contains(&format!("os error {}", EXDEV))
}

pub fn remove_any(path: &Path) -> Result<(), FleaError> {
    let meta = path
        .symlink_metadata()
        .map_err(|e| from_io("move", &path.to_string_lossy(), &e))?;
    let r = if meta.is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    r.map_err(|e| from_io("move", &path.to_string_lossy(), &e))
}

fn cancel_err(path: &Path) -> FleaError {
    FleaError {
        where_: "copy".to_string(),
        path: path.to_string_lossy().to_string(),
        msg: "cancelled".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::sync::atomic::AtomicBool;

    fn quiet<'a>(flag: &'a AtomicBool, sink: &'a mut dyn FnMut(u64, u64)) -> Progress<'a> {
        Progress { cancel: flag, on_bytes: sink, partial: None }
    }

    #[test]
    fn a_file_copy_reproduces_the_bytes_and_reports_progress() {
        let d = TestDir::new("copyfile");
        let src = d.file("src.bin", "0123456789");
        let flag = AtomicBool::new(false);
        let mut seen: Vec<(u64, u64)> = Vec::new();
        let mut sink = |done: u64, total: u64| seen.push((done, total));
        copy_any(&src, &d.join("dst.bin"), &mut quiet(&flag, &mut sink)).expect("copy");
        assert_eq!(std::fs::read_to_string(d.join("dst.bin")).unwrap(), "0123456789");
        assert_eq!(seen.last().copied(), Some((10, 10)), "the last report is the whole file");
    }

    #[test]
    fn a_copy_refuses_to_overwrite_an_existing_destination() {
        let d = TestDir::new("copyclobber");
        let src = d.file("src.txt", "new");
        d.file("dst.txt", "already here");
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        let e = copy_any(&src, &d.join("dst.txt"), &mut quiet(&flag, &mut sink)).expect_err("must refuse");
        assert_eq!(e.where_, "copy");
        assert_eq!(std::fs::read_to_string(d.join("dst.txt")).unwrap(), "already here");
    }

    #[test]
    fn a_symlink_is_copied_as_a_symlink_and_never_followed() {
        let d = TestDir::new("copylink");
        d.file("target.txt", "target body");
        let link = d.join("link.txt");
        std::os::unix::fs::symlink("target.txt", &link).unwrap();
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        copy_any(&link, &d.join("copied.txt"), &mut quiet(&flag, &mut sink)).expect("copy");
        let meta = d.join("copied.txt").symlink_metadata().unwrap();
        assert!(meta.file_type().is_symlink(), "following it would silently turn a link into a file");
        assert_eq!(
            std::fs::read_link(d.join("copied.txt")).unwrap(),
            std::path::PathBuf::from("target.txt")
        );
    }

    #[test]
    fn a_directory_is_copied_with_its_tree_and_its_links() {
        let d = TestDir::new("copytree");
        let src = d.dir("tree");
        std::fs::write(src.join("a.txt"), "a").unwrap();
        std::fs::create_dir(src.join("sub")).unwrap();
        std::fs::write(src.join("sub/b.txt"), "b").unwrap();
        std::os::unix::fs::symlink("a.txt", src.join("link")).unwrap();
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        copy_any(&src, &d.join("clone"), &mut quiet(&flag, &mut sink)).expect("copy");
        assert_eq!(std::fs::read_to_string(d.join("clone/a.txt")).unwrap(), "a");
        assert_eq!(std::fs::read_to_string(d.join("clone/sub/b.txt")).unwrap(), "b");
        assert!(d.join("clone/link").symlink_metadata().unwrap().file_type().is_symlink());
    }

    #[test]
    fn a_cancelled_copy_leaves_no_partial_file_behind() {
        let d = TestDir::new("copycancel");
        let src = d.file("big.bin", &"x".repeat(CHUNK * 3));
        let flag = AtomicBool::new(true);
        let mut sink = |_: u64, _: u64| {};
        let e = copy_any(&src, &d.join("partial.bin"), &mut quiet(&flag, &mut sink)).expect_err("cancelled");
        assert_eq!(e.msg, "cancelled");
        assert!(!d.join("partial.bin").exists(), "a half-written destination is not a result");
    }

    #[test]
    fn a_cancelled_directory_copy_removes_the_part_it_already_wrote() {
        let d = TestDir::new("copydircancel");
        let src = d.dir("tree");
        std::fs::write(src.join("a.bin"), "x".repeat(8)).unwrap();
        std::fs::write(src.join("b.bin"), "y".repeat(8)).unwrap();
        let clone = d.join("clone");
        let flag = AtomicBool::new(false);
        // Cancels on the second file's first chunk, whichever file read_dir yields second, so one
        // complete file is in the tree when it is cut. A cancel during the first file only tests an
        // empty directory: copy_file removes its own partial first, and remove_dir would pass too.
        let mut chunks = 0;
        let mut names_at_cancel: Vec<String> = Vec::new();
        let mut sink = |_done: u64, _total: u64| {
            chunks += 1;
            if chunks == 2 {
                flag.store(true, Ordering::Relaxed);
                names_at_cancel = std::fs::read_dir(&clone)
                    .unwrap()
                    .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
                    .collect();
            }
        };
        let e = copy_any(&src, &clone, &mut quiet(&flag, &mut sink)).expect_err("cancelled");
        assert_eq!(e.msg, "cancelled");
        assert_eq!(
            names_at_cancel.len(),
            2,
            "the tree held one complete file and the one being cut when the cancel landed, got {:?}",
            names_at_cancel
        );
        assert!(!clone.exists(), "a half-copied tree is not a result, and no journal step records one");
    }

    // The second entry's destination is taken from under it while the first is still streaming, so
    // the failure is a create that collides and not a cancel, whichever order read_dir yields.
    #[test]
    fn a_directory_copy_that_fails_short_of_a_cancel_keeps_the_tree_and_reports_it() {
        let d = TestDir::new("copydirfail");
        let src = d.dir("tree");
        std::fs::write(src.join("a.bin"), "x".repeat(8)).unwrap();
        std::fs::write(src.join("b.bin"), "y".repeat(16)).unwrap();
        let clone = d.join("clone");
        let flag = AtomicBool::new(false);
        let mut planted = false;
        let mut sink = |_done: u64, total: u64| {
            if planted {
                return;
            }
            planted = true;
            let other = if total == 8 { "b.bin" } else { "a.bin" };
            std::fs::write(clone.join(other), "stray").unwrap();
        };
        let mut p = quiet(&flag, &mut sink);
        let e = copy_any(&src, &clone, &mut p).expect_err("the second entry collides");
        assert_ne!(e.msg, "cancelled");
        assert_eq!(p.partial, Some(clone.clone()), "the tree is reported as the partial to journal");
        let a = std::fs::read_to_string(clone.join("a.bin")).unwrap();
        let b = std::fs::read_to_string(clone.join("b.bin")).unwrap();
        assert!(
            (a == "x".repeat(8) && b == "stray") || (b == "y".repeat(16) && a == "stray"),
            "the file that landed before the failure is complete and still there, got a={:?} b={:?}",
            a,
            b
        );
    }

    // A directory opens read-only like a file on Linux and then fails the first read with EISDIR,
    // which is a failure after the destination was created and not before.
    #[test]
    fn a_file_copy_that_fails_after_creating_its_destination_reports_the_partial() {
        let d = TestDir::new("copyfilefail");
        let src = d.dir("not-a-file");
        let dst = d.join("partial.bin");
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        let mut p = quiet(&flag, &mut sink);
        let e = copy_file(&src, &dst, 0, &mut p).expect_err("a directory cannot be read as a file");
        assert_ne!(e.msg, "cancelled");
        assert!(dst.exists(), "the partial stays: removing it on an error is the cancel path's job only");
        assert_eq!(p.partial, Some(dst));
    }

    // Nothing was created, so nothing is reported: a step here would let undo delete what the user had.
    #[test]
    fn a_copy_refused_because_the_destination_exists_reports_no_partial() {
        let d = TestDir::new("copynopartial");
        let src = d.file("src.txt", "new");
        let taken_file = d.file("taken.txt", "already here");
        let src_dir = d.dir("tree");
        let taken_dir = d.dir("taken");
        std::fs::write(taken_dir.join("keep.txt"), "keep").unwrap();
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        let mut p = quiet(&flag, &mut sink);
        copy_any(&src, &taken_file, &mut p).expect_err("must refuse");
        assert!(p.partial.is_none());
        copy_any(&src_dir, &taken_dir, &mut p).expect_err("must refuse");
        assert!(p.partial.is_none());
        assert_eq!(std::fs::read_to_string(&taken_file).unwrap(), "already here");
        assert_eq!(std::fs::read_to_string(taken_dir.join("keep.txt")).unwrap(), "keep");
    }

    #[test]
    fn a_same_filesystem_move_leaves_nothing_at_the_source() {
        let d = TestDir::new("movesame");
        let src = d.file("moving.txt", "body");
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        move_any(&src, &d.join("moved.txt"), &mut quiet(&flag, &mut sink)).expect("move");
        assert!(!src.exists());
        assert_eq!(std::fs::read_to_string(d.join("moved.txt")).unwrap(), "body");
    }

    #[test]
    fn a_move_onto_an_existing_name_refuses_and_keeps_the_source() {
        let d = TestDir::new("moveclobber");
        let src = d.file("a.txt", "source");
        d.file("b.txt", "destination");
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        move_any(&src, &d.join("b.txt"), &mut quiet(&flag, &mut sink)).expect_err("must refuse");
        assert!(src.exists(), "the source is untouched when the move is refused");
        assert_eq!(std::fs::read_to_string(d.join("b.txt")).unwrap(), "destination");
    }
}
