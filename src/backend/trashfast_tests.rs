// A trash request renames an item on the home Trash's own filesystem in-process; only the rest reach gio and its two lists.
use super::*;
use crate::backend::hometrash::{percent_encode, renames_to_home, set_test_trash_root, test_set_other_filesystem, test_take_folder_syncs, test_vanish_on_rename, tests::gio_like};
use crate::backend::testdir::TestDir;
use std::cell::RefCell;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::rc::Rc;

// Clears this thread's Trash root, filesystem seam and gio stand-in on the way out, so a failed assert leaks nothing.
struct Sandbox;
impl Drop for Sandbox {
    fn drop(&mut self) {
        set_test_trash_root(None);
        test_set_other_filesystem(false);
        test_vanish_on_rename(None);
        STAND_IN.with(|slot| *slot.borrow_mut() = None);
    }
}

// Gives a folder its mode back before the sandbox is removed.
struct Mode(PathBuf);
impl Drop for Mode {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
    }
}

// gio played faithfully over the sandbox Trash, recording every call so a test can count the spawns.
fn counted_gio(d: &TestDir, xdg: &Path) -> (Sandbox, Rc<RefCell<Vec<String>>>) {
    set_test_trash_root(Some(xdg.join("Trash")));
    test_reset_list_calls();
    test_take_folder_syncs();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let (seen, can, held) = (Rc::clone(&calls), d.dir("can"), xdg.to_path_buf());
    STAND_IN.with(|slot| *slot.borrow_mut() = Some(Rc::new(move |args: &[&str]| {
        seen.borrow_mut().push(args.join(" "));
        Some(gio_like(&can, &held, args))
    })));
    (Sandbox, calls)
}

// The Path= value of an info file, after checking its header and its local DeletionDate.
fn recorded_path(info: &Path) -> String {
    let body = std::fs::read_to_string(info).unwrap();
    assert!(body.starts_with("[Trash Info]\n"), "the info file opens with its section header: {body}");
    let date = body.lines().find_map(|l| l.strip_prefix("DeletionDate=")).expect("the info file stamps the deletion date");
    assert_eq!((date.len(), &date[4..5], &date[10..11]), (19, "-", "T"), "DeletionDate is local YYYY-MM-DDTHH:MM:SS: {date}");
    body.lines().find_map(|l| l.strip_prefix("Path=")).expect("the info file names the original path").to_string()
}

fn count(calls: &Rc<RefCell<Vec<String>>>) -> usize {
    calls.borrow().len()
}

#[test]
fn a_file_and_a_folder_on_the_trash_filesystem_spawn_no_gio_and_undo_restores_both() {
    let d = TestDir::new("trash-fast");
    let xdg = d.dir("xdg");
    d.dir("home");
    let file = d.file("home/my file \u{e9}.txt", &"x".repeat(1024));
    let folder = d.dir("home/album");
    d.file("home/album/a.png", "inside");
    let (_sandbox, calls) = counted_gio(&d, &xdg);
    let (ok, failed, reason) = trash_checked(&[file.clone(), folder.clone()], None).expect("a present batch is not an error");
    assert_eq!((ok.len(), failed, reason.as_str()), (2, 0, ""));
    assert_eq!(count(&calls), 0, "no gio process runs: {:?}", calls.borrow());
    assert_eq!(test_list_calls(), 0, "and no Trash is listed");
    assert_eq!(test_take_folder_syncs(), (1, 3), "the request syncs the files folder, the info folder and the parent once");
    assert_eq!((ok[0].original.as_path(), ok[1].original.as_path()), (file.as_path(), folder.as_path()), "entries keep the request order");
    assert_eq!(ok[0].uri, "trash:///my%20file%20%C3%A9.txt", "the URI names the reserved file");
    assert_eq!(ok[1].uri, "trash:///album");
    assert!(file.symlink_metadata().is_err() && folder.symlink_metadata().is_err(), "both left their place");
    assert_eq!(std::fs::read(xdg.join("Trash/files/my file \u{e9}.txt")).unwrap().len(), 1024);
    assert_eq!(std::fs::read_to_string(xdg.join("Trash/files/album/a.png")).unwrap(), "inside");
    assert_eq!(recorded_path(&xdg.join("Trash/info/my file \u{e9}.txt.trashinfo")), percent_encode(file.as_os_str().as_bytes()), "Path is the percent-encoded original");
    assert_eq!(recorded_path(&xdg.join("Trash/info/album.trashinfo")), percent_encode(folder.as_os_str().as_bytes()));
    for entry in &ok {
        restore(entry).expect("undo restores a home Trash entry");
    }
    assert_eq!(std::fs::read(&file).unwrap().len(), 1024, "the file is back byte for byte in size");
    assert_eq!(std::fs::read_to_string(folder.join("a.png")).unwrap(), "inside", "the folder is back whole");
    assert_eq!(std::fs::read_dir(xdg.join("Trash/files")).unwrap().count() + std::fs::read_dir(xdg.join("Trash/info")).unwrap().count(), 0, "the Trash holds nothing again");
    assert_eq!(count(&calls), 0, "undo spawned no gio either: {:?}", calls.borrow());
}

#[test]
fn two_files_of_one_name_trashed_in_turn_get_two_entries() {
    let d = TestDir::new("trash-fast-twice");
    let xdg = d.dir("xdg");
    d.dir("home");
    let (_sandbox, calls) = counted_gio(&d, &xdg);
    let path = d.file("home/report.txt", "first");
    let (first, _, _) = trash_checked(std::slice::from_ref(&path), None).unwrap();
    d.file("home/report.txt", "second");
    let (second, _, _) = trash_checked(std::slice::from_ref(&path), None).unwrap();
    assert_eq!((first[0].uri.as_str(), second[0].uri.as_str()), ("trash:///report.txt", "trash:///report.2.txt"));
    assert_eq!(std::fs::read_to_string(xdg.join("Trash/files/report.txt")).unwrap(), "first");
    assert_eq!(std::fs::read_to_string(xdg.join("Trash/files/report.2.txt")).unwrap(), "second");
    assert_eq!(recorded_path(&xdg.join("Trash/info/report.2.txt.trashinfo")), percent_encode(path.as_os_str().as_bytes()));
    restore(&second[0]).expect("the newer entry restores by its own URI");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "second", "the second trash came back, not the first");
    assert_eq!(std::fs::read_to_string(xdg.join("Trash/files/report.txt")).unwrap(), "first", "the first stays in the Trash");
    assert_eq!(count(&calls), 0, "{:?}", calls.borrow());
}

#[test]
fn a_path_on_another_filesystem_goes_through_gio_exactly_as_before() {
    let d = TestDir::new("trash-fast-otherfs");
    let xdg = d.dir("xdg");
    d.dir("usb");
    let path = d.file("usb/photo.png", "yours");
    let (_sandbox, calls) = counted_gio(&d, &xdg);
    test_set_other_filesystem(true);
    let (ok, failed, reason) = trash_checked(std::slice::from_ref(&path), None).unwrap();
    assert_eq!((ok.len(), failed, reason.as_str()), (1, 0, ""));
    assert_eq!(test_take_folder_syncs(), (0, 0), "a request with no rename syncs no folder");
    let calls = calls.borrow();
    assert_eq!(calls.iter().filter(|c| *c == "trash --list").count(), 2, "under today's before and after lists: {calls:?}");
    assert_eq!(calls.iter().filter(|c| c.starts_with("trash -- ")).count(), 1, "gio trashes the item: {calls:?}");
    assert!(ok[0].uri.starts_with("trash:///"), "the entry came off gio's list: {}", ok[0].uri);
    assert!(!xdg.join("Trash").exists(), "the home Trash is never touched in-process");
}

#[test]
fn a_batch_of_both_kinds_runs_gio_for_the_path_that_needs_it_alone() {
    let d = TestDir::new("trash-fast-mixed");
    let xdg = d.dir("xdg");
    d.dir("home");
    let quick = d.file("home/quick.txt", "q");
    // An item inside the Trash itself is gio's, which knows its own rules for it.
    d.dir("xdg/Trash/files");
    let inside = d.file("xdg/Trash/files/slow.txt", "s");
    let (_sandbox, calls) = counted_gio(&d, &xdg);
    let (ok, failed, reason) = trash_checked(&[quick.clone(), inside.clone()], None).unwrap();
    assert_eq!((ok.len(), failed, reason.as_str()), (2, 0, ""));
    assert_eq!((ok[0].original.as_path(), ok[1].original.as_path()), (quick.as_path(), inside.as_path()), "entries keep the request order");
    assert_eq!(ok[0].uri, "trash:///quick.txt");
    let calls = calls.borrow();
    assert_eq!(calls.iter().filter(|c| *c == "trash --list").count(), 2, "{calls:?}");
    assert_eq!(calls.iter().filter(|c| c.starts_with("trash -- ")).collect::<Vec<_>>(), vec![&format!("trash -- {}", inside.display())], "gio is handed the one path it needs: {calls:?}");
}

#[test]
fn a_rename_that_fails_leaves_no_orphan_info_file_and_falls_back_to_gio() {
    let d = TestDir::new("trash-fast-refused");
    let xdg = d.dir("xdg");
    d.dir("home");
    let path = d.file("home/photo.png", "yours");
    d.dir("xdg/Trash/info");
    let files = d.dir("xdg/Trash/files");
    // The info folder takes the record and the files folder refuses the item, the way a full or foreign folder would.
    std::fs::set_permissions(&files, std::fs::Permissions::from_mode(0o500)).unwrap();
    let _mode = Mode(files);
    let (_sandbox, calls) = counted_gio(&d, &xdg);
    let (ok, failed, reason) = trash_checked(std::slice::from_ref(&path), None).unwrap();
    assert_eq!((ok.len(), failed, reason.as_str()), (1, 0, ""), "gio still trashes it");
    assert!(ok[0].uri.starts_with("trash:///"), "{}", ok[0].uri);
    assert_eq!(std::fs::read_dir(xdg.join("Trash/info")).unwrap().count(), 0, "no info file stays reserved");
    let calls = calls.borrow();
    assert_eq!(calls.iter().filter(|c| c.starts_with("trash -- ")).count(), 1, "the path went down the gio path: {calls:?}");
    assert_eq!(calls.iter().filter(|c| *c == "trash --list").count(), 2, "under the before and after lists: {calls:?}");
}

#[test]
fn a_path_gone_before_the_call_stays_a_failure_beside_a_renamed_one() {
    let d = TestDir::new("trash-fast-gone");
    let xdg = d.dir("xdg");
    d.dir("home");
    let kept = d.file("home/kept.txt", "k");
    let gone = d.path().join("home/gone.txt");
    let (_sandbox, calls) = counted_gio(&d, &xdg);
    let (ok, failed, reason) = trash_checked(&[gone, kept], None).unwrap();
    assert_eq!((ok.len(), failed, reason.as_str()), (1, 1, "those items are already gone"));
    assert_eq!(count(&calls), 0, "{:?}", calls.borrow());
}

#[test]
fn a_path_gone_between_the_sort_and_its_rename_is_a_failure_that_never_reaches_gio() {
    let d = TestDir::new("trash-fast-vanished");
    let xdg = d.dir("xdg");
    d.dir("home");
    let kept = d.file("home/kept.txt", "k");
    let vanishing = d.file("home/vanishing.txt", "v");
    let (_sandbox, calls) = counted_gio(&d, &xdg);
    test_vanish_on_rename(Some(vanishing.clone()));
    let (ok, failed, reason) = trash_checked(&[vanishing, kept.clone()], None).unwrap();
    assert_eq!((ok.len(), failed, reason.as_str()), (1, 1, "those items are already gone"));
    assert_eq!(ok[0].original, kept, "the item still there is the one trashed");
    assert_eq!(count(&calls), 0, "the vanished path is not handed to gio: {:?}", calls.borrow());
    assert_eq!(test_list_calls(), 0, "and no Trash is listed");
    let infos: Vec<_> = std::fs::read_dir(xdg.join("Trash/info")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(infos, vec![std::ffi::OsString::from("kept.txt.trashinfo")], "the refused rename left no info file");
}

#[test]
fn a_relative_path_is_never_renamed_in_process() {
    let d = TestDir::new("trash-fast-relative");
    let here = std::env::current_dir().unwrap();
    let (_sandbox, _calls) = counted_gio(&d, &here.join("flea-test-no-such-xdg"));
    // Same folder and same device as the Trash root, so only the absolute check tells the two apart.
    assert!(renames_to_home(&here.join("flea-test-no-such-file")), "the absolute twin is renameable");
    assert!(!renames_to_home(Path::new("./flea-test-no-such-file")), "the relative path is left to gio");
}

#[test]
fn an_item_that_is_its_own_mount_is_left_to_gio_though_its_parent_shares_the_trash_device() {
    use std::os::unix::fs::MetadataExt;
    let d = TestDir::new("trash-fast-mount");
    let (_sandbox, _calls) = counted_gio(&d, Path::new("/dev/flea-test-no-such-xdg"));
    // /dev/shm is a mount of its own below /dev, and the /dev/fd symlink sits on /dev itself (a container's /dev is a tmpfs, so /dev/null is not).
    let (shm, fd) = (Path::new("/dev/shm"), Path::new("/dev/fd"));
    let device = |p: &Path| p.symlink_metadata().ok().map(|m| m.dev());
    let dev = device(Path::new("/dev"));
    if dev.is_none() || device(shm).is_none() || device(shm) == dev {
        eprintln!("skipped the mount leg: this host does not mount /dev/shm apart from /dev");
        return;
    }
    assert_eq!(device(fd), dev, "the leg needs /dev/fd itself on /dev's device, a fact of the host and not of the trash code");
    assert!(renames_to_home(fd), "an item on the parent's device is renameable");
    assert!(!renames_to_home(shm), "a mount point under the same parent is left to gio");
}

// Trashes five files in-process and journals them as one trash step each, as a trash request does.
fn trashed_five(d: &TestDir) -> (Vec<PathBuf>, crate::backend::undo::Entry) {
    use crate::backend::undo::Step;
    d.dir("home");
    let make = |i: usize| d.file(&format!("home/f{i}.txt"), &format!("body {i}"));
    let paths: Vec<PathBuf> = (0..5).map(make).collect();
    let (ok, failed, _) = trash_checked(&paths, None).unwrap();
    assert_eq!((ok.len(), failed), (5, 0));
    test_take_folder_syncs();
    let steps = ok.into_iter().map(Step::Trashed).collect();
    let op = "trash".to_string();
    (paths, crate::backend::undo::Entry { op, steps })
}

#[test]
fn undo_of_a_trash_batch_syncs_each_folder_once_and_not_three_per_item() {
    let d = TestDir::new("trash-fast-undo-syncs");
    let xdg = d.dir("xdg");
    let (_sandbox, calls) = counted_gio(&d, &xdg);
    for shared in [false, true] {
        let (paths, entry) = trashed_five(&d);
        let mut journal = crate::backend::undo::Journal::new();
        if shared {
            journal.attach_test_shared(d.dir("runtime"));
        }
        journal.push(entry);
        assert_eq!(journal.undo().unwrap(), "trash");
        for (i, path) in paths.iter().enumerate() {
            assert_eq!(std::fs::read_to_string(path).unwrap(), format!("body {i}"));
        }
        // One pass over the parent, files/ and info/, with the shared journal and without.
        assert_eq!(test_take_folder_syncs(), (1, 3), "shared journal: {shared}");
        let records = std::fs::read_dir(xdg.join("Trash/info")).unwrap().count();
        assert_eq!(records, 0);
    }
    assert_eq!(count(&calls), 0, "{:?}", calls.borrow());
}

#[test]
fn an_undo_that_stops_on_a_taken_name_still_syncs_what_it_restored() {
    let d = TestDir::new("trash-fast-undo-taken");
    let xdg = d.dir("xdg");
    let (_sandbox, _calls) = counted_gio(&d, &xdg);
    let (paths, entry) = trashed_five(&d);
    // Undo walks the steps newest first, so f4..f2 come back before the taken f1 stops it.
    d.file("home/f1.txt", "someone else");
    let mut journal = crate::backend::undo::Journal::new();
    journal.push(entry);
    let refused = journal.undo().unwrap_err();
    assert_eq!(refused.msg, crate::backend::homerestore::RESTORE_TAKEN);
    assert!(paths[2..].iter().all(|p| p.is_file()) && !paths[0].exists());
    assert_eq!(std::fs::read_to_string(&paths[1]).unwrap(), "someone else");
    // The restores before the refusal were still synced, once, on the way out.
    assert_eq!(test_take_folder_syncs(), (1, 3));
}
