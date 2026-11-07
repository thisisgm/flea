// A new folder takes its name in one rename once every entry in it landed; a dead copy's sibling goes on the next transfer.
use crate::backend::collide::tests::owned;
use crate::backend::collide::Policy;
use crate::backend::copyfile::{copy_any, move_any};
use crate::backend::copymanifest::writer_for;
use crate::backend::copystage;
use crate::backend::copystage_tests::{body, in_place, parts, quiet, source, BODY};
use crate::backend::opscancel::Live;
use crate::backend::opsreq::{run_transfer_checked, OpMsg};
use crate::backend::testdir::TestDir;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;

const ENOSPC: i32 = 28;

// A folder of two big files and a nested one, so a copy reports progress mid-tree.
fn tree(d: &TestDir) -> PathBuf {
    let tree = d.dir("from/album");
    std::fs::create_dir(tree.join("inner")).unwrap();
    std::fs::write(tree.join("a.bin"), body()).unwrap();
    std::fs::write(tree.join("inner").join("b.bin"), body()).unwrap();
    tree
}

fn landed_whole(album: &Path) -> bool {
    std::fs::read(album.join("a.bin")).is_ok_and(|b| b == body()) && std::fs::read(album.join("inner/b.bin")).is_ok_and(|b| b == body())
}

// Mid-copy the name is absent and one hidden folder fills, each file under its own name inside it; then one rename.
#[test]
fn a_new_folder_is_built_hidden_and_takes_its_name_once_whole() {
    let d = TestDir::new("folder-whole");
    let src = tree(&d);
    let to = d.dir("dest");
    let dst = to.join("album");
    let flag = AtomicBool::new(false);
    let mut seen: Vec<(bool, Vec<String>, bool)> = Vec::new();
    let mut sink = |_: u64, _: u64| {
        let hidden = parts(&to);
        let streaming_in_place = hidden.first().is_some_and(|h| to.join(h).join("a.bin").exists() || to.join(h).join("inner/b.bin").exists());
        seen.push((dst.exists(), hidden, streaming_in_place));
    };
    copy_any(&src, &dst, &mut quiet(&flag, &mut sink)).expect("the folder lands");
    assert!(seen.len() >= 4, "progress came mid-tree: {seen:?}");
    assert!(seen.iter().all(|(named, hidden, in_place)| !named && hidden.len() == 1 && *in_place), "{seen:?}");
    assert!(landed_whole(&dst));
    assert!(parts(&to).is_empty() && parts(&dst).is_empty() && parts(&dst.join("inner")).is_empty());
}

// A cancel or a failure short of one mid-tree leaves neither the name nor the hidden folder, and nothing to journal.
#[test]
fn a_cancelled_or_failed_folder_leaves_nothing() {
    for cancel in [true, false] {
        let d = TestDir::new("folder-fail");
        let src = tree(&d);
        let to = d.dir("dest");
        let dst = to.join("album");
        let flag = AtomicBool::new(false);
        // Four writes land one whole file, then the fifth fails on the next file's first chunk.
        let mut sink = |_: u64, _: u64| if cancel { flag.store(true, Ordering::Relaxed) };
        if !cancel {
            crate::backend::copyfile::test_fail_write(Some((4, ENOSPC)));
        }
        let mut p = quiet(&flag, &mut sink);
        let e = copy_any(&src, &dst, &mut p).expect_err("the folder copy ends early");
        crate::backend::copyfile::test_fail_write(None);
        assert_eq!(e.msg == "cancelled", cancel, "{}", e.msg);
        assert!(p.partial.is_none(), "nothing is left for undo, cancel={cancel}");
        assert_eq!(std::fs::read_dir(&to).unwrap().count(), 0, "cancel={cancel}");
        assert!(landed_whole(&src), "the source stands whole");
    }
}

// A name another writer takes mid-copy wins: the folder copy refuses as the exclusive create did and leaves nothing.
#[test]
fn a_folder_name_taken_mid_copy_is_kept_and_the_copy_refuses() {
    let d = TestDir::new("folder-conflict");
    let src = tree(&d);
    let to = d.dir("dest");
    let dst = to.join("album");
    let flag = AtomicBool::new(false);
    let mut taken = false;
    let mut sink = |_: u64, _: u64| {
        if !taken {
            std::fs::create_dir(&dst).expect("the name is still free mid-copy");
            std::fs::write(dst.join("theirs.txt"), "theirs").unwrap();
            taken = true;
        }
    };
    let mut p = quiet(&flag, &mut sink);
    let e = copy_any(&src, &dst, &mut p).expect_err("the name is taken");
    assert!(e.msg.contains("exists"), "the refusal names the taken name: {}", e.msg);
    assert_eq!(std::fs::read_dir(&dst).unwrap().count(), 1, "the other folder is untouched");
    assert!(parts(&to).is_empty() && p.partial.is_none());
}

// A failed folder that will not go is the partial; its own manifest goes with it, and a merge's wider one stays.
#[test]
fn a_failed_folder_that_will_not_go_drops_only_its_own_manifest() {
    for own in [true, false] {
        let d = TestDir::new("folder-stuck");
        let src = tree(&d);
        let to = d.dir("dest");
        let dst = to.join("album");
        let flag = AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        let mut p = quiet(&flag, &mut sink);
        p.manifest = if own { writer_for(&src, &dst) } else { writer_for(&d.join("from"), &to) };
        crate::backend::copyfile::test_stuck_folder();
        crate::backend::copyfile::test_fail_write(Some((4, ENOSPC)));
        copy_any(&src, &dst, &mut p).expect_err("the fifth write fails");
        crate::backend::copyfile::test_fail_write(None);
        let stuck = parts(&to);
        assert_eq!(p.partial, Some(to.join(&stuck[0])), "the sibling that stayed is the partial, own={own}");
        assert_eq!(p.manifest.is_some(), !own, "own={own}");
    }
}

// A move across filesystems is a copy and a delete, so its folder is staged the same way and the source goes after.
#[test]
fn a_cross_device_folder_move_stages_and_removes_the_source_after() {
    let d = TestDir::new("folder-move");
    let src = tree(&d);
    let to = d.dir("dest");
    let dst = to.join("album");
    crate::backend::copyfile::test_force_exdev(true);
    let flag = AtomicBool::new(false);
    let (mut named, mut calls) = (false, 0);
    let mut sink = |_: u64, _: u64| (named, calls) = (named || dst.exists(), calls + 1);
    let moved = move_any(&src, &dst, &mut quiet(&flag, &mut sink));
    crate::backend::copyfile::test_force_exdev(false);
    moved.expect("the folder moves");
    assert!(calls > 0 && !named, "the move copied, and the name stayed absent while the folder filled: {calls}");
    assert!(landed_whole(&dst) && !src.exists() && parts(&to).is_empty());
}

// A destination that writes in place (FUSE) builds the folder under its name, the explicit fallback, as before.
#[test]
fn a_folder_on_a_destination_that_writes_in_place_fills_under_its_name() {
    let _seams = in_place();
    let d = TestDir::new("folder-inplace");
    let src = tree(&d);
    let to = d.dir("dest");
    let dst = to.join("album");
    let flag = AtomicBool::new(false);
    let (mut named, mut calls) = (true, 0);
    let mut sink = |_: u64, _: u64| (named, calls) = (named && dst.exists(), calls + 1);
    copy_any(&src, &dst, &mut quiet(&flag, &mut sink)).expect("the folder lands");
    assert!(calls > 0 && named && landed_whole(&dst) && parts(&to).is_empty());
}

// A quit and the end of input cancel the running operation as they are read, before the loop, as Cancel does.
#[test]
fn a_quit_and_the_end_of_input_cancel_the_running_copy_at_the_door() {
    let live = Live::new();
    let flag = Arc::new(AtomicBool::new(false));
    let job = Arc::new(AtomicBool::new(false));
    live.claim(7, &flag);
    live.1.insert(9, &job);
    let _ = crate::backend::events::read_line(r#"{"c":"list","path":"/quit"}"#.to_string(), &live);
    assert!(!flag.load(Ordering::Relaxed) && !job.load(Ordering::Relaxed), "a line that only mentions quit cancels nothing");
    let _ = crate::backend::events::read_line(r#"{"c": "quit"}"#.to_string(), &live);
    assert!(flag.load(Ordering::Relaxed), "the quit set the slot operation's own flag");
    assert!(job.load(Ordering::Relaxed), "the quit set the copy job's own flag");
    let (tx, _rx) = crate::backend::events::channel();
    let again = Arc::new(AtomicBool::new(false));
    let job_again = Arc::new(AtomicBool::new(false));
    live.claim(8, &again);
    live.1.insert(10, &job_again);
    let mut tail = Vec::new();
    assert!(!crate::backend::events::feed(&mut tail, &[], true, &tx, &live), "the end of input closes");
    assert!(again.load(Ordering::Relaxed), "the end of input set the slot operation's own flag");
    assert!(job_again.load(Ordering::Relaxed), "the end of input set the copy job's own flag");
}

// A pid this boot no longer runs: a child that has already been reaped.
fn dead_pid() -> u32 {
    let mut child = std::process::Command::new("/bin/true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

// A crash's leftover file or folder goes on the next transfer into its folder; a live copy's sibling and any chosen name stay.
#[test]
fn the_next_transfer_sweeps_a_dead_copys_siblings_and_nothing_else() {
    use std::os::unix::fs::PermissionsExt;
    let d = TestDir::new("stage-sweep");
    let to = d.dir("dest");
    let dead = to.join(copystage::test_leaf_for(dead_pid(), 3));
    let other_boot = to.join(crate::backend::copysweep::test_foreign_leaf(4242, 1));
    let mine = to.join(copystage::test_leaf_for(std::process::id(), 99_999));
    let mut sleeper = std::process::Command::new("/bin/sleep").arg("30").spawn().unwrap();
    let live = to.join(copystage::test_leaf_for(sleeper.id(), 1));
    for path in [&dead, &other_boot, &mine, &live] {
        std::fs::write(path, "half").unwrap();
    }
    // A dead copy's folder, with a shut directory inside as a copied 0500 source leaves it.
    let dead_dir = to.join(copystage::test_leaf_for(dead_pid(), 4));
    std::fs::create_dir_all(dead_dir.join("shut")).unwrap();
    std::fs::write(dead_dir.join("shut/a.bin"), "half").unwrap();
    std::fs::set_permissions(dead_dir.join("shut"), std::fs::Permissions::from_mode(0o500)).unwrap();
    let outside = d.dir("outside").join("keep.txt");
    std::fs::write(&outside, "keep").unwrap();
    std::os::unix::fs::symlink(outside.parent().unwrap(), dead_dir.join("link")).unwrap();
    let live_dir = to.join(copystage::test_leaf_for(sleeper.id(), 2));
    std::fs::create_dir(&live_dir).unwrap();
    let lookalikes = [".flea-part-notes.txt", ".flea-part-0000000g-1-1", ".flea-part-00000000-1-1.bak", "flea-part-00000000-1-1"];
    for name in lookalikes {
        std::fs::write(to.join(name), "mine").unwrap();
    }
    let link_named = to.join(copystage::test_leaf_for(dead_pid(), 5));
    std::os::unix::fs::symlink("/nonexistent", &link_named).unwrap();
    let src = source(&d);
    let (tx, rx) = channel();
    run_transfer_checked(1, false, owned(&[&src]), to.clone(), Arc::new(AtomicBool::new(false)), tx, None, None, Policy::default());
    let _ = sleeper.kill();
    let _ = sleeper.wait();
    assert!(rx.iter().any(|m| matches!(m, OpMsg::Item { ok: true, .. })), "the transfer itself landed");
    assert!(!dead.exists() && !dead_dir.exists(), "a dead copy's file and folder went");
    assert!(outside.exists(), "a link inside the folder went without its target");
    assert!(!other_boot.exists(), "another boot's sibling on a local drive went");
    assert!(mine.exists() && live.exists() && live_dir.is_dir(), "a running copy's siblings stay");
    for name in lookalikes {
        assert!(to.join(name).exists(), "{name} is not a sibling name and stays");
    }
    assert!(link_named.symlink_metadata().is_ok(), "a link is never swept");
    assert_eq!(std::fs::read(to.join("VID_big.mp4")).unwrap().len(), BODY);
}

// On a share another machine may be writing the same folder, so another boot's sibling waits out its age there.
#[test]
fn another_boots_sibling_on_a_share_goes_only_once_it_is_old() {
    use crate::backend::copysweep::{test_stale_foreign, test_stale_foreign_after, test_unchanged_for};
    let d = TestDir::new("stage-shared");
    let young = d.file("young", "half");
    let meta = std::fs::metadata(&young).unwrap();
    assert!(!test_stale_foreign(&meta, true), "a fresh one may be a copy still running elsewhere");
    assert!(test_stale_foreign(&meta, false), "on a local drive another boot cannot be running it");
    let backdated = d.file("backdated", "half");
    let two_hours = std::time::Duration::from_secs(2 * 60 * 60);
    std::fs::File::options().write(true).open(&backdated).unwrap().set_modified(std::time::SystemTime::now() - two_hours).unwrap();
    assert!(!test_stale_foreign(&std::fs::metadata(&backdated).unwrap(), true), "a live copy's sibling carries the source's old mtime");
    assert!(test_stale_foreign_after(Some(two_hours), true), "one left unchanged that long goes");
    let folder = d.dir("folder");
    assert!(test_unchanged_for(&std::fs::metadata(&folder).unwrap()).is_none(), "a folder's ctime never ages it out on a share");
    assert!(!test_stale_foreign_after(None, true) && test_stale_foreign_after(None, false), "so on a share it stays, on a local drive it goes");
}
