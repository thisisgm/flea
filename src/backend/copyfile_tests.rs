use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::sync::atomic::AtomicBool;

fn quiet<'a>(flag: &'a AtomicBool, sink: &'a mut dyn FnMut(u64, u64)) -> Progress<'a> {
    Progress { cancel: flag, on_bytes: sink, partial: None, tree: None }
}

// copy_any sends a symlink to copy_symlink, so a symlink reaching copy_file was swapped in after
// that stat. Without O_NOFOLLOW this copies the target's bytes, which is the defect.
#[test]
fn a_source_swapped_to_a_symlink_after_the_stat_is_refused_rather_than_followed() {
    let d = TestDir::new("nofollow");
    let secret = d.file("secret.txt", "not yours");
    let src = d.join("src.bin");
    std::os::unix::fs::symlink(&secret, &src).expect("the swap the stat cannot see");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let e = copy_file(&src, &d.join("dst.bin"), 9, &mut quiet(&flag, &mut sink))
        .expect_err("a symlinked source must not be followed");
    assert_eq!(e.where_, "copy");
    assert!(!d.join("dst.bin").exists(), "and nothing of the target reached the destination");
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

// The Copying card sat still for a whole tree because a directory item reported nothing at all:
// no name, no bytes, an empty bar. What a tree can report without a sweep is its running count.
#[test]
fn a_directory_copy_reports_one_running_count_for_the_whole_tree() {
    let d = TestDir::new("copytreebytes");
    let src = d.dir("tree");
    std::fs::write(src.join("a.bin"), "a".repeat(10)).unwrap();
    std::fs::create_dir(src.join("sub")).unwrap();
    std::fs::write(src.join("sub/b.bin"), "b".repeat(4)).unwrap();
    let flag = AtomicBool::new(false);
    let mut seen: Vec<(u64, u64)> = Vec::new();
    let mut sink = |done: u64, total: u64| seen.push((done, total));
    copy_any(&src, &d.join("clone"), &mut quiet(&flag, &mut sink)).expect("copy");
    assert!(seen.iter().all(|(_, total)| *total == 0), "a tree claims no total, got {:?}", seen);
    let counts: Vec<u64> = seen.iter().map(|(done, _)| *done).collect();
    assert!(counts.windows(2).all(|pair| pair[1] > pair[0]), "the count only ever rises, got {:?}", counts);
    assert_eq!(counts.last().copied(), Some(14), "and ends at the bytes the tree really holds");
}

// Issue 109 (gardnmi): under umask 022 a 0600 file landed 0644 and a 0700 directory 0755, so a copy
// published what the original kept private. A copy is never more permissive than its source.
#[test]
fn a_copy_keeps_the_private_modes_of_what_it_copied() {
    let d = TestDir::new("copymodes");
    let src = d.dir("tree");
    std::fs::write(src.join("secret.txt"), "s").unwrap();
    std::fs::set_permissions(src.join("secret.txt"), std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o700)).unwrap();
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    copy_any(&src, &d.join("clone"), &mut quiet(&flag, &mut sink)).expect("copy");
    let dir_mode = d.join("clone").symlink_metadata().unwrap().permissions().mode() & 0o777;
    let file_mode = d.join("clone/secret.txt").symlink_metadata().unwrap().permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700, "the directory's own mode is carried");
    assert_eq!(file_mode, 0o600, "and so is the file's");
    assert_eq!(file_mode & !0o600, 0, "a copy is never more permissive than its source");
}

// The umask narrows and never widens: a group and world writable source lands without those bits
// wherever the process umask withholds them, which is what a fresh create has always done.
#[test]
fn a_copy_takes_no_bit_the_umask_withholds() {
    let d = TestDir::new("copyumask");
    let src = d.file("open.txt", "o");
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o666)).unwrap();
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    copy_any(&src, &d.join("open-copy.txt"), &mut quiet(&flag, &mut sink)).expect("copy");
    let mode = d.join("open-copy.txt").symlink_metadata().unwrap().permissions().mode() & 0o777;
    let control = d.join("control.txt");
    std::fs::OpenOptions::new().write(true).create_new(true).mode(0o666).open(&control).unwrap();
    let fresh = control.symlink_metadata().unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, fresh, "what a fresh create at the source's bits lands at under this umask");
    assert_eq!(mode & !0o666, 0, "and never a bit the source did not carry");
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

// Issue 110: the tree's own parent is renamed aside between two children and a symlink to somebody
// else's directory is left at its name. Resolving each child from the path again reads the second file
// through that symlink, so private bytes land in the destination.
#[test]
fn a_parent_swapped_between_two_children_cannot_redirect_the_copy() {
    let d = TestDir::new("copyswap");
    let src = d.dir("selected");
    std::fs::write(src.join("a.txt"), "public a").unwrap();
    std::fs::write(src.join("b.txt"), "public b").unwrap();
    let outside = d.dir("outside");
    std::fs::write(outside.join("a.txt"), "PRIVATE").unwrap();
    std::fs::write(outside.join("b.txt"), "PRIVATE").unwrap();
    let flag = AtomicBool::new(false);
    let swapped = std::cell::Cell::new(false);
    // The swap lands after the first child's bytes, which is the window the issue's own fixture uses.
    let mut sink = |_: u64, _: u64| {
        if !swapped.replace(true) {
            std::fs::rename(&src, d.join("moved")).expect("the rename an attacker makes");
            std::os::unix::fs::symlink(&outside, &src).expect("the symlink left at its name");
        }
    };
    let clone = d.join("clone");
    // The held descriptors outlive the rename, so the copy finishes from the directory that moved.
    copy_any(&src, &clone, &mut quiet(&flag, &mut sink)).expect("the copy reads on through the swap");
    for (name, public) in [("a.txt", "public a"), ("b.txt", "public b")] {
        let landed = std::fs::read_to_string(clone.join(name)).expect("the child the selection named");
        assert_eq!(landed, public, "{name} was read through the replacement symlink");
    }
}

#[test]
fn a_cancelled_copy_leaves_no_partial_file_behind() {
    let d = TestDir::new("copycancel");
    let src = d.file("big.bin", &"x".repeat(CHUNK * 3));
    let flag = AtomicBool::new(true);
    let mut sink = |_: u64, _: u64| {};
    d.assert_contains(&d.join("partial.bin"));
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
    d.assert_contains(&clone);
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
// the failure is a create that collides and not a cancel, whichever order read_dir yields. A tree
// copy reports against the tree's running total, not the file's size, so the file being cut is the
// one destination that exists when the first bytes land; the other one is the one to take. Reading
// the size off the callback found it only on tmpfs, where read_dir yields the newer name first.
#[test]
fn a_directory_copy_that_fails_short_of_a_cancel_keeps_the_tree_and_reports_it() {
    let d = TestDir::new("copydirfail");
    let src = d.dir("tree");
    std::fs::write(src.join("a.bin"), "x".repeat(8)).unwrap();
    std::fs::write(src.join("b.bin"), "y".repeat(16)).unwrap();
    let clone = d.join("clone");
    let flag = AtomicBool::new(false);
    let mut planted = false;
    let mut sink = |_done: u64, _total: u64| {
        if planted {
            return;
        }
        planted = true;
        let other = if clone.join("a.bin").exists() { "b.bin" } else { "a.bin" };
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

// /proc/self/mem is S_IFREG and opens fine, and its first read at offset 0 answers EIO, so the
// failure lands after the destination was created rather than before.
#[test]
fn a_file_copy_that_fails_after_creating_its_destination_reports_the_partial() {
    let d = TestDir::new("copyfilefail");
    let src = std::path::PathBuf::from("/proc/self/mem");
    let dst = d.join("partial.bin");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink);
    let e = copy_file(&src, &dst, 0, &mut p).expect_err("offset 0 of a mem file is not mapped");
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
    d.assert_contains(&src);
    d.assert_contains(&d.join("moved.txt"));
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
    d.assert_contains(&src);
    d.assert_contains(&d.join("b.txt"));
    let error = move_any(&src, &d.join("b.txt"), &mut quiet(&flag, &mut sink)).expect_err("must refuse");
    assert_eq!(error.where_, "rename");
    assert_eq!(error.path, d.join("b.txt").to_string_lossy());
    assert_eq!(error.msg, "already exists");
    assert!(src.exists(), "the source is untouched when the move is refused");
    assert_eq!(std::fs::read_to_string(d.join("b.txt")).unwrap(), "destination");
}

// F7 of the C1 copy review: a 0500 source directory is copied to a 0500 destination, so every later
// write into it, the cancel path's own removal included, has to put the owner's bits back first.
#[test]
fn a_source_directory_the_owner_cannot_write_is_copied_with_that_mode() {
    let d = TestDir::new("copyreadonlydir");
    let src = d.dir("tree");
    let sub = d.dir("tree/sub");
    std::fs::write(sub.join("f.txt"), "s").unwrap();
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o500)).unwrap();
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let clone = d.join("clone");
    copy_any(&src, &clone, &mut quiet(&flag, &mut sink)).expect("copy");
    let mode = clone.join("sub").symlink_metadata().unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o500, "the source's own mode is carried, write bit and all");
    assert_eq!(std::fs::read_to_string(clone.join("sub/f.txt")).unwrap(), "s");
}

#[test]
fn a_cancel_removes_a_tree_holding_a_directory_the_copy_made_unwritable() {
    let d = TestDir::new("copyreadonlycancel");
    let src = d.dir("tree");
    // Both children are 0500, so whichever read_dir yields first is complete and unwritable by the
    // time the second one's bytes raise the cancel, whatever order the filesystem hands them back.
    for name in ["one", "two"] {
        let sub = d.dir(&format!("tree/{}", name));
        std::fs::write(sub.join("f.bin"), "x".repeat(8)).unwrap();
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o500)).unwrap();
    }
    let clone = d.join("clone");
    let flag = AtomicBool::new(false);
    let mut chunks = 0;
    let mut sink = |_: u64, _: u64| {
        chunks += 1;
        if chunks == 2 {
            flag.store(true, Ordering::Relaxed);
        }
    };
    d.assert_contains(&clone);
    let e = copy_any(&src, &clone, &mut quiet(&flag, &mut sink)).expect_err("cancelled");
    assert_eq!(e.msg, "cancelled");
    assert!(!clone.exists(), "a half-copied tree goes, even when the copy left it unwritable");
}
