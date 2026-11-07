use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::sync::atomic::AtomicBool;

fn quiet<'a>(flag: &'a AtomicBool, sink: &'a mut dyn FnMut(u64, u64)) -> Progress<'a> {
    Progress { cancel: flag, on_bytes: sink, partial: None, tree: None, manifest: None, durability: None, for_move: false }
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

// A d_type-regular child skips the lstat, so copy_file_at's O_NOFOLLOW open alone must refuse a symlink swapped in.
#[test]
fn a_child_swapped_to_a_symlink_is_refused_by_the_file_open_itself() {
    let d = TestDir::new("treefileswap");
    let secret = d.file("secret.txt", "not yours");
    let src = d.join("swapped.bin");
    std::os::unix::fs::symlink(&secret, &src).expect("the swap the listing cannot see");
    let dst = d.join("dst.bin");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink);
    let e = copy_file_at(here(&src), here(&dst), 0, &mut p, false)
        .expect_err("a symlinked child must not be followed");
    assert_eq!(e.where_, "copy");
    assert!(!dst.exists(), "and nothing of the target reached the destination");
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

// A copy keeps the source mtime, best effort, so a copied tree still sorts by the files' own history.
#[test]
fn a_copy_keeps_the_source_mtime() {
    let d = TestDir::new("copymtime");
    let src = d.file("src.bin", "0123456789");
    let status = std::process::Command::new("touch").arg("-d").arg("2001-02-03 04:05:06").arg(&src).status().expect("touch sets the source mtime");
    assert!(status.success(), "touch must run for this pin");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    copy_any(&src, &d.join("dst.bin"), &mut quiet(&flag, &mut sink)).expect("copy");
    let src_mtime = src.metadata().unwrap().mtime();
    let dst_mtime = d.join("dst.bin").metadata().unwrap().mtime();
    assert_eq!(dst_mtime, src_mtime, "the copy carries the source mtime, not the copy time");
}

// A source filesystem with no real modes lends no exec bits: exfat and ntfs take the umask default.
#[test]
fn a_modeless_source_takes_umask_modes() {
    assert_eq!(mode_for_source(Some(0x2011BAB0), 0o755, false) & 0o111, 0, "an exfat file lends no exec bit");
    assert_eq!(mode_for_source(Some(0x5346544E), 0o777, false) & 0o111, 0, "an ntfs file lends no exec bit");
    let umask = super::umask();
    let want = 0o777 & !umask & 0o111;
    assert_eq!(mode_for_source(Some(0x7366746e), 0o777, true) & 0o111, want, "an ntfs3 dir keeps the umask's exec bits");
    assert_ne!(mode_for_source(Some(0xEF53), 0o755, false) & 0o111, 0, "an ext4 file keeps its exec bit");
    assert_ne!(mode_for_source(None, 0o755, false) & 0o111, 0, "an unknown filesystem keeps its exec bit");
}

// Sample input: a tree with a.txt and link->a.txt copied under test_force_linkless(true).
#[test]
fn a_linkless_folder_copy_names_the_link_and_keeps_the_folder_mtime() {
    let d = TestDir::new("linklessfolder");
    let src = d.dir("tree");
    std::fs::write(src.join("a.txt"), "a").unwrap();
    std::os::unix::fs::symlink("a.txt", src.join("link")).unwrap();
    let status = std::process::Command::new("touch").arg("-d").arg("2001-02-03 04:05:06").arg(&src).status().expect("touch sets the source dir mtime");
    assert!(status.success(), "touch must run for this pin");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    crate::backend::unheld::test_force_linkless(true);
    let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None, tree: None, manifest: None, durability: None, for_move: false };
    let outcome = copy_any(&src, &d.join("clone"), &mut p);
    let refused = crate::backend::unheld::take();
    crate::backend::unheld::test_force_linkless(false);
    let error = outcome.expect_err("a folder that lost its link is not a whole copy");
    assert_eq!(error.msg, "link could not be copied: this drive cannot hold links");
    assert_eq!(refused, vec![(d.join("clone/link"), "this drive cannot hold links".to_string())], "the link is named");
    assert_eq!(std::fs::read_to_string(d.join("clone/a.txt")).unwrap(), "a");
    assert!(d.join("clone/link").symlink_metadata().is_err(), "the link is nowhere, counted and not copied");
    let src_mtime = src.metadata().unwrap().mtime();
    let dst_mtime = d.join("clone").metadata().unwrap().mtime();
    assert_eq!(dst_mtime, src_mtime, "the folder carries the source mtime too");
}

// Sample input: test_set_precheck(Some(0), Some(0), None) lets a 3-byte copy_any through.
#[test]
fn a_precheck_with_unknown_room_lets_copy_any_through() {
    let d = TestDir::new("precheckunknown");
    let src = d.file("src.txt", "abc");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    test_set_precheck(Some(0), Some(0), None);
    let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None, tree: None, manifest: None, durability: None, for_move: false };
    let outcome = copy_any(&src, &d.join("dst.txt"), &mut p);
    test_set_precheck(None, None, None);
    outcome.expect("unknown room never refuses");
    assert_eq!(std::fs::read_to_string(d.join("dst.txt")).unwrap(), "abc");
    test_set_precheck(Some(2), Some(100), None);
    let mut q = Progress { cancel: &flag, on_bytes: &mut sink, partial: None, tree: None, manifest: None, durability: None, for_move: false };
    let error = copy_any(&src, &d.join("dst2.txt"), &mut q).expect_err("no room still refuses");
    test_set_precheck(None, None, None);
    assert_eq!(error.msg, "dst2.txt needs 3 B, and the drive has 2 B free", "the room refusal names the file and both sizes");
}

// A 4 GiB file never starts onto vfat, and a file bigger than the free room never starts anywhere, so both refuse up front.
#[test]
fn a_copy_refuses_a_file_the_destination_cannot_hold() {
    assert_eq!(refuse_for_size("big.iso", Some(VFAT_MAGIC), u64::MAX, 100, FOUR_GIB).unwrap(),
        "a file of 4 GiB or more does not fit on a FAT32 drive");
    assert!(refuse_for_size("big.iso", Some(VFAT_MAGIC), u64::MAX, 100, FOUR_GIB - 1).is_none());
    assert!(refuse_for_size("big.iso", Some(0xEF53), u64::MAX, 100, FOUR_GIB).is_none(), "ext4 has no 4 GiB file limit");
    assert_eq!(refuse_for_size("photo.heic", None, 1_100_000_000, 100, 4_200_000_000).unwrap(),
        "photo.heic needs 4.2 GB, and the drive has 1.1 GB free", "the room refusal leads with the name and reads sizes");
    assert!(refuse_for_size("a.txt", None, 101, 100, 101).is_none(), "a file that exactly fits is let through");
}

// A statfs with zero total blocks reports unknown room, so a gvfs mount that omits free never refuses.
#[test]
fn a_statfs_with_zero_blocks_is_unknown_room() {
    assert!(refuse_for_size("a.txt", None, 0, 0, 1).is_none(), "zero blocks skips the free test");
    assert!(refuse_for_size("a.txt", Some(0xEF53), 0, 0, 4096).is_none(), "unknown room holds any size");
    assert_eq!(refuse_for_size("big.iso", Some(VFAT_MAGIC), 0, 0, FOUR_GIB).unwrap(),
        "a file of 4 GiB or more does not fit on a FAT32 drive");
    assert!(refuse_for_size("a.txt", None, 0, 100, 1).is_some(), "zero free on a real filesystem still refuses");
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
    // In place, as a FUSE destination writes a tree; a staged folder is covered in copyfolder_tests.
    let _seams = crate::backend::copystage_tests::in_place();
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
// the failure is a name that collides and not a cancel, whichever order read_dir yields.
#[test]
fn a_directory_copy_that_fails_short_of_a_cancel_keeps_the_tree_and_reports_it() {
    // In place, as a FUSE destination writes a tree; a staged folder goes whole instead (copyfolder_tests).
    let _seams = crate::backend::copystage_tests::in_place();
    let d = TestDir::new("copydirfail");
    let src = d.dir("tree");
    std::fs::write(src.join("a.bin"), "x".repeat(8)).unwrap();
    std::fs::write(src.join("b.bin"), "y".repeat(16)).unwrap();
    let clone = d.join("clone");
    let flag = AtomicBool::new(false);
    let mut planted = false;
    let mut sink = |done: u64, _total: u64| {
        if planted {
            return;
        }
        planted = true;
        // The streaming file is still under its hidden name, so its first report's size says which one it is.
        let other = if done == 8 { "b.bin" } else { "a.bin" };
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

// A failed folder confirm keeps the source, so a crash mid-move never loses the file.
#[test]
fn a_cross_device_move_confirms_the_destination_before_removing_the_source() {
    let d = TestDir::new("movexdev");
    let src = d.file("moving.txt", "body");
    let dst = d.join("moved.txt");
    crate::backend::durable::test_reset();
    crate::backend::durable::test_mark_durable(d.path());
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut durability = crate::backend::durable::Durability::begin(&dst);
    assert!(durability.durable, "the marked sandbox is a durable destination");
    let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None, tree: None, manifest: None, durability: Some(&mut durability), for_move: false };
    crate::backend::durable::test_set_fail_dirs(true);
    let error = move_cross_device(&src, &dst, &mut p).expect_err("a failed folder confirm must not remove the source");
    assert_eq!(error.msg, crate::backend::durable::DIR_UNCONFIRMED);
    assert!(src.exists(), "the source stays when the destination folders did not confirm");
    assert!(dst.exists(), "the landed copy stays beside it");
    assert_eq!(p.partial.as_deref(), Some(dst.as_path()), "the unconfirmed copy is handed to the caller to journal");
    crate::backend::durable::test_reset();
}

// Without a durability context there is no touched set, so the file's own parent is confirmed.
#[test]
fn a_context_free_cross_device_move_confirms_the_parent_before_removing_the_source() {
    let d = TestDir::new("movexdevplain");
    let src = d.file("moving.txt", "body");
    let dst = d.join("moved.txt");
    crate::backend::durable::test_reset();
    crate::backend::durable::test_set_fail_dirs(true);
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let error = move_cross_device(&src, &dst, &mut quiet(&flag, &mut sink)).expect_err("a failed parent fsync must not remove the source");
    assert_eq!(error.msg, crate::backend::durable::DIR_UNCONFIRMED);
    assert!(src.exists(), "the source stays when the parent did not confirm");
    crate::backend::durable::test_reset();
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

// A batch single-item move settles its held file before removing its source.
#[test]
fn a_batch_cross_device_move_settles_before_removing_its_source() {
    let d = TestDir::new("movexdevbatch");
    let src = d.file("moving.txt", "body");
    let dst = d.join("moved.txt");
    crate::backend::durable::test_reset();
    crate::backend::durable::test_mark_durable(d.path());
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut durability = crate::backend::durable::Durability::begin(&dst);
    durability.batch_syncfs = true;
    let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None, tree: None, manifest: None, durability: Some(&mut durability), for_move: false };
    move_cross_device(&src, &dst, &mut p).expect("a batch move confirms");
    let order = crate::backend::durable::test_order();
    let syncfs_at = order.iter().position(|s| s == "syncfs").expect("one syncfs in the order");
    let dir_at = order.iter().position(|s| s == "dir").expect("one folder fsync in the order");
    let release_at = order.iter().position(|s| s == "release").expect("held file closes");
    assert!(syncfs_at < release_at && release_at < dir_at, "syncfs, release, folder fsync, then source removal: {:?}", order);
    assert!(!src.exists(), "the source goes only after the confirm");
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "body");
    crate::backend::durable::test_reset();
}

// A failing batch syncfs keeps the source of a single-item move, so a crash never loses the file.
#[test]
fn a_batch_cross_device_move_keeps_its_source_when_syncfs_fails() {
    let d = TestDir::new("movexdevbatchfail");
    let src = d.file("moving.txt", "body");
    let dst = d.join("moved.txt");
    crate::backend::durable::test_reset();
    crate::backend::durable::test_mark_durable(d.path());
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut durability = crate::backend::durable::Durability::begin(&dst);
    durability.batch_syncfs = true;
    let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None, tree: None, manifest: None, durability: Some(&mut durability), for_move: false };
    crate::backend::durable::test_set_syncfs_errno(crate::backend::durable::EIO);
    let error = move_cross_device(&src, &dst, &mut p).expect_err("an unconfirmed batch move keeps its source");
    crate::backend::durable::test_set_syncfs_errno(0);
    assert_eq!(error.msg, crate::backend::durable::DIR_UNCONFIRMED);
    assert!(src.exists(), "the source stays: {:?}", error.msg);
    assert!(dst.exists(), "the landed copy stays beside it");
    assert_eq!(p.partial.as_deref(), Some(dst.as_path()), "the unconfirmed copy is handed to the caller to journal");
    assert_eq!(crate::backend::durable::test_syncfs_count(), 0, "a failed syncfs never counts: {:?}", crate::backend::durable::test_order());
    crate::backend::durable::test_reset();
}

// A tagged file copy lands tagged, which the journal test alone never proved.
#[test]
fn a_file_copy_carries_the_source_tags() {
    let d = TestDir::new("tagscopyfile");
    let src = d.file("src.txt", "body");
    crate::backend::tags::write_names(&src, &["Red".to_string()]).unwrap();
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    copy_any(&src, &d.join("dst.txt"), &mut quiet(&flag, &mut sink)).expect("copy");
    assert_eq!(crate::backend::tags::read(&d.join("dst.txt")).unwrap(), vec!["Red"]);
}

// A tagged directory copy lands tagged on the directory and on the child inside it.
#[test]
fn a_directory_copy_carries_the_tree_tags() {
    let d = TestDir::new("tagscopydir");
    let tree = d.dir("tree");
    let inner = tree.join("a.txt");
    std::fs::write(&inner, "a").expect("a.txt");
    crate::backend::tags::write_names(&tree, &["Green".to_string()]).unwrap();
    crate::backend::tags::write_names(&inner, &["Red".to_string()]).unwrap();
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let clone = d.join("tree-copy");
    copy_any(&tree, &clone, &mut quiet(&flag, &mut sink)).expect("copy");
    assert_eq!(crate::backend::tags::read(&clone).unwrap(), vec!["Green"]);
    assert_eq!(crate::backend::tags::read(&clone.join("a.txt")).unwrap(), vec!["Red"]);
}

// A same-device move is a rename, so the tags ride the inode to the new name.
#[test]
fn a_same_device_move_keeps_the_tags() {
    let d = TestDir::new("tagsmove");
    let src = d.file("src.txt", "body");
    crate::backend::tags::write_names(&src, &["Red".to_string()]).unwrap();
    let dst = d.join("dst.txt");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    move_any(&src, &dst, &mut quiet(&flag, &mut sink)).expect("move");
    assert_eq!(crate::backend::tags::read(&dst).unwrap(), vec!["Red"]);
    assert!(!src.exists(), "the rename leaves no source behind");
}

// A cross-device move copies first, so the tags must arrive on the copy.
#[test]
fn a_cross_device_move_carries_the_tags() {
    let d = TestDir::new("tagsmoveexdev");
    let src = d.file("src.txt", "body");
    crate::backend::tags::write_names(&src, &["Blue".to_string()]).unwrap();
    let dst = d.join("dst.txt");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    test_force_exdev(true);
    let outcome = move_any(&src, &dst, &mut quiet(&flag, &mut sink));
    test_force_exdev(false);
    outcome.expect("move");
    assert_eq!(crate::backend::tags::read(&dst).unwrap(), vec!["Blue"]);
    assert!(!src.exists(), "the copy-then-remove leaves no source behind");
}

// A twin move through move_any keeps the stranded message, so a redo and a transfer item name the temp leaf.
#[test]
fn a_twin_move_through_move_any_names_its_temp() {
    let d = TestDir::new("movetwinstranded");
    let from = d.file("a.txt", "body");
    let twin = d.join("A.txt");
    std::fs::hard_link(&from, &twin).unwrap();
    crate::backend::renamecompat::test_fail_twin_back();
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink);
    let error = move_any(&from, &twin, &mut p).expect_err("a failed move-back strands the file");
    assert_eq!(error.where_, "rename-stranded", "a stranded twin answers its own kind");
    assert!(error.msg.starts_with("the file was left as .flea-case-"), "msg names temp leaf: {}", error.msg);
    let leaf = error.msg.split(" as ").nth(1).unwrap().split(" in ").next().unwrap();
    assert!(d.path().join(leaf).is_file(), "the temp holds the file");
}

// Forces fuse.rclone's EINVAL answer to RENAME_NOREPLACE on a directory (issue 282) and its copy fallback.
fn rclone_move(src: &Path, dst: &Path, p: &mut Progress) -> Result<(), FleaError> {
    crate::backend::renamecompat::test_force_fallback(true);
    crate::backend::renamecompat::test_force_copy_fallback(true);
    let outcome = move_any(src, dst, p);
    crate::backend::renamecompat::test_force_copy_fallback(false);
    crate::backend::renamecompat::test_force_fallback(false);
    outcome
}

// Two files and a subfolder holding a third, the shape a folder move must carry whole.
fn small_tree(d: &TestDir) -> std::path::PathBuf {
    let src = d.dir("tree");
    std::fs::write(src.join("a.txt"), "alpha").unwrap();
    std::fs::write(src.join("b.txt"), "beta").unwrap();
    std::fs::create_dir(src.join("sub")).unwrap();
    std::fs::write(src.join("sub/c.txt"), "gamma").unwrap();
    src
}

fn assert_small_tree(root: &Path) {
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "alpha");
    assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "beta");
    assert_eq!(std::fs::read_to_string(root.join("sub/c.txt")).unwrap(), "gamma");
}

#[test]
fn a_directory_move_takes_the_copy_fallback_when_noreplace_answers_einval() {
    let d = TestDir::new("movefusefallback");
    let src = small_tree(&d);
    let dst = d.join("moved");
    d.assert_contains(&src);
    d.assert_contains(&dst);
    let flag = AtomicBool::new(false);
    let mut seen = Vec::new();
    let mut sink = |done: u64, total: u64| seen.push((done, total));
    rclone_move(&src, &dst, &mut quiet(&flag, &mut sink)).expect("an EINVAL noreplace still moves through the copy fallback");
    assert!(!seen.is_empty(), "the copy reports its bytes to the transfer's progress");
    assert!(!src.exists(), "the source goes only once the destination is whole");
    assert_small_tree(&dst);
}

// The fallback keeps no-clobber: an existing destination refuses and both trees stay untouched.
#[test]
fn a_fallback_directory_move_onto_an_existing_name_refuses_and_keeps_both_trees() {
    let d = TestDir::new("movefuseclobber");
    let src = small_tree(&d);
    let dst = d.dir("taken");
    std::fs::write(dst.join("keep.txt"), "keep").unwrap();
    d.assert_contains(&src);
    d.assert_contains(&dst);
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let error = rclone_move(&src, &dst, &mut quiet(&flag, &mut sink)).expect_err("the fallback must refuse a taken name");
    assert_eq!(error.msg, "already exists");
    assert_eq!(error.where_, "copy", "the refusal comes from the copy fallback, not the plain rename");
    assert_small_tree(&src);
    assert_eq!(std::fs::read_to_string(dst.join("keep.txt")).unwrap(), "keep");
    assert!(dst.join("a.txt").symlink_metadata().is_err(), "nothing of the source lands under the taken name");
}
