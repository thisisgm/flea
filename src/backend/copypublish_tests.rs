// Where a staged copy meets its edges: a refused publish, a kernel without RENAME_NOREPLACE, a sibling that will not go, in place.
use crate::backend::copyfile::{copy_any, test_set_precheck};
use crate::backend::copystage;
use crate::backend::copystage_tests::{body, parts, quiet, sibling_only, source, Seams};
use crate::backend::testdir::TestDir;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const EIO: i32 = 5;

// A drive that refuses the publish itself fails the item and keeps nothing; the source is untouched.
#[test]
fn a_refused_publish_fails_the_item_and_keeps_nothing() {
    for sibling in [false, true] {
        let _seams = if sibling { sibling_only() } else { Seams };
        let d = TestDir::new("stage-refused");
        let src = source(&d);
        let to = d.dir("dest");
        copystage::test_fail_publish(Some(1));
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        copy_any(&src, &to.join("VID_big.mp4"), &mut quiet(&flag, &mut sink)).expect_err("EPERM on the publish fails the item");
        assert_eq!(std::fs::read_dir(&to).unwrap().count(), 0, "sibling={sibling}");
        assert_eq!(std::fs::read(&src).unwrap(), body());
    }
}

// A kernel without RENAME_NOREPLACE (nfs) lands the sibling through the checked fallback, a hard link first.
#[test]
fn a_drive_without_noreplace_lands_through_the_link_fallback() {
    let _seams = sibling_only();
    let d = TestDir::new("stage-noreplace");
    let src = source(&d);
    let to = d.dir("dest");
    let dst = to.join("VID_big.mp4");
    crate::backend::renamecompat::test_force_fallback(true);
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let landed = copy_any(&src, &dst, &mut quiet(&flag, &mut sink));
    crate::backend::renamecompat::test_force_fallback(false);
    landed.expect("the fallback lands the file");
    assert_eq!(std::fs::read(&dst).unwrap(), body());
    assert!(parts(&to).is_empty());
    // The link's own failure surfacing pins that branch, since a plain rename would land the file all the same.
    let again = to.join("again.mp4");
    crate::backend::renamecompat::test_force_fallback(true);
    crate::backend::renamecompat::test_fail_hard_link(Some(EIO));
    let linked = copy_any(&src, &again, &mut quiet(&flag, &mut sink));
    crate::backend::renamecompat::test_force_fallback(false);
    crate::backend::renamecompat::test_fail_hard_link(None);
    linked.expect_err("the fallback went through the link, whose EIO fails the item");
    assert!(!again.exists());
    assert!(parts(&to).is_empty());
}

// A sibling whose removal fails stays, and the failure names it for the journal, so undo can still remove it.
#[test]
fn a_sibling_that_will_not_go_is_reported_as_the_partial() {
    let _seams = sibling_only();
    let d = TestDir::new("stage-stuck");
    let to = d.dir("dest");
    copystage::test_fail_discard(Some(EIO));
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = quiet(&flag, &mut sink);
    copy_any(Path::new("/proc/self/mem"), &to.join("mem.bin"), &mut p).expect_err("offset 0 of a mem file is not mapped");
    let stuck = parts(&to);
    assert_eq!(stuck.len(), 1, "the sibling that would not go stays");
    assert_eq!(p.partial, Some(to.join(&stuck[0])), "and is the partial the journal records");
    assert!(!to.join("mem.bin").exists(), "the name was never taken");
}

// A FUSE destination, or one whose statfs failed, writes in place, the explicit fallback, and cancels as before.
#[test]
fn a_destination_that_writes_in_place_copies_and_cancels_as_before() {
    for unknown in [false, true] {
        let _seams = Seams;
        if unknown {
            test_set_precheck(Some(u64::MAX), Some(1), None);
        } else {
            copystage::test_in_place(true);
        }
        let d = TestDir::new("stage-inplace");
        let src = source(&d);
        let to = d.dir("dest");
        let dst = to.join("VID_big.mp4");
        let flag = AtomicBool::new(false);
        let mut named = false;
        let mut sink = |_: u64, _: u64| named |= dst.exists();
        copy_any(&src, &dst, &mut quiet(&flag, &mut sink)).expect("the copy lands");
        assert!(named, "in place, the name fills as it copies, unknown={unknown}");
        assert_eq!(std::fs::read(&dst).unwrap(), body());
        std::fs::remove_file(&dst).unwrap();
        let mut sink = |_: u64, _: u64| flag.store(true, Ordering::Relaxed);
        copy_any(&src, &dst, &mut quiet(&flag, &mut sink)).expect_err("cancelled");
        assert!(!dst.exists(), "the cancel still removes what it wrote");
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        let mut p = quiet(&flag, &mut sink);
        let mem = to.join("mem.bin");
        copy_any(Path::new("/proc/self/mem"), &mem, &mut p).expect_err("offset 0 of a mem file is not mapped");
        assert_eq!(p.partial, Some(mem), "a failure in place keeps its half for the journal, as before");
    }
}
