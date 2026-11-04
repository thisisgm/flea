// FsInfo's tests, beside it so fsinforeq.rs stays under the file budget.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Receiver;
use crate::backend::listing::Listing;

const SHARE: &str = "/run/user/1000/gvfs/smb-share:server=fake,share=media";

fn share_dir(name: &str) -> PathBuf {
    Path::new(SHARE).join(name)
}

fn figures(free: u64) -> Option<Info> {
    Some(Info { name: "fuse".to_string(), free, blocks: 100 })
}

// The GUI, which asks for figures on every listing, so its listings start the share's statfs.
fn gui(mut fs: FsInfo) -> FsInfo {
    fs.asked = true;
    fs
}

// A fake statfs that takes delay_ms and counts its calls.
fn counted(delay_ms: u64, free: u64) -> (FsInfo, Receiver<Event>, Arc<AtomicUsize>) {
    let (events, rx) = crate::backend::events::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let reader: Reader = Arc::new(move |_: PathBuf| {
        seen.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(delay_ms));
        (figures(free), Some(0x65735546))
    });
    (gui(FsInfo::with_reader(events, reader)), rx, calls)
}

fn next_done(rx: &Receiver<Event>) -> Done {
    match rx.recv_timeout(Duration::from_secs(5)).expect("the worker reports its figures") {
        Event::FsInfo(done) => done,
        _ => panic!("the worker reports its figures as an fsinfo event"),
    }
}

#[test]
fn a_client_that_never_asks_for_figures_starts_no_statfs() {
    let (events, _rx) = crate::backend::events::channel();
    let mut fs = FsInfo::with_reader(events, Arc::new(|_: PathBuf| (figures(7), None)));
    fs.list_arrived(&share_dir("a"), &Listing::new());
    assert!(fs.inflight.is_empty(), "the TUI lists a share and never asks for its figures, so no statfs starts");
    let _ = fs.answer(&share_dir("a"));
    assert_eq!(fs.inflight.len(), 1, "the first ask starts the share's statfs");
    fs.list_arrived(&share_dir("b"), &Listing::new());
    assert!(fs.waiting.contains(&gvfs_root(&share_dir("b")).unwrap()), "once asked, a listing asks for its share again");
}

#[test]
fn a_slow_fsinfo_answers_its_class_at_once_and_its_figures_later() {
    let (mut fs, rx, _) = counted(500, 7);
    let dir = share_dir("photos");
    fs.list_arrived(&dir, &Listing::new());
    let t = Instant::now();
    let (info, class, _) = fs.answer(&dir);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    assert!(info.is_none(), "figures the worker has not returned are unknown, never a block");
    assert_eq!(class, "network");
    assert!(ms < 250.0, "answered in {:.1} ms against a 500 ms statfs, over a 250 ms budget", ms);
    let done = next_done(&rx);
    assert_eq!((done.root.as_path(), done.dir.as_path()), (Path::new(SHARE), dir.as_path()), "keyed by the share, read at the folder");
    assert_eq!(fs.finish(done, &dir).flatten().map(|i| i.free), Some(7), "the current folder's figures print");
    let (info, _, _) = fs.answer(&dir);
    assert_eq!(info.map(|i| i.free), Some(7), "the folder answers its known figures at once next time");
    let (info, _, _) = fs.answer(&share_dir("other"));
    assert!(info.is_none(), "another folder never borrows figures it was not read for");
}

#[test]
fn one_statfs_per_share_at_a_time_and_the_folder_on_screen_goes_next() {
    let (mut fs, rx, calls) = counted(200, 7);
    fs.list_arrived(&share_dir("a"), &Listing::new());
    fs.list_arrived(&share_dir("b"), &Listing::new());
    fs.list_arrived(&share_dir("c"), &Listing::new());
    let done = next_done(&rx);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "three folders of one share ran {} statfs calls at once", calls.load(Ordering::SeqCst));
    assert!(fs.finish(done, &share_dir("c")).is_none(), "a's figures never print over c");
    let second = next_done(&rx);
    assert_eq!(second.dir, share_dir("c"), "the skipped middle folder is never read, the one on screen is");
    assert_eq!(fs.finish(second, &share_dir("c")).flatten().map(|i| i.free), Some(7));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn unchanged_figures_print_nothing_and_moved_ones_print() {
    let (mut fs, _rx, _) = counted(0, 7);
    let dir = share_dir("a");
    let done = |free| Done { root: PathBuf::from(SHARE), dir: share_dir("a"), seq: 0, info: figures(free), tagable: true };
    assert!(fs.finish(done(7), &dir).is_some(), "the first figures print");
    assert!(fs.finish(done(7), &dir).is_none(), "the answer already carried these");
    assert!(fs.finish(done(5), &dir).is_some(), "a copy to the share moved them");
}

#[test]
fn a_share_that_refuses_tags_prints_again_and_answers_false_after() {
    let (mut fs, _rx, _) = counted(0, 7);
    let dir = share_dir("a");
    let done = |tagable| Done { root: PathBuf::from(SHARE), dir: share_dir("a"), seq: 0, info: figures(7), tagable };
    assert!(fs.tagable(&dir, "network"), "a share answers true until its worker's probe lands");
    assert!(fs.finish(done(true), &dir).is_some(), "the first figures print");
    assert!(fs.finish(done(false), &dir).is_some(), "a refusal with the same figures still prints");
    assert!(!fs.tagable_shown(&dir) && !fs.tagable(&dir, "network"), "the worker's refusal is the answer now");
    assert!(fs.tagable_shown(&share_dir("b")), "another folder starts from true again");
}

#[test]
fn a_folder_the_client_has_left_prints_nothing() {
    let (mut fs, _rx, _) = counted(0, 7);
    let done = Done { root: PathBuf::from(SHARE), dir: share_dir("back"), seq: 0, info: figures(7), tagable: true };
    assert!(fs.finish(done, Path::new("/home/gm")).is_none(), "a left folder's figures name the wrong place");
    let (info, _, _) = fs.answer_in(&share_dir("back"), "");
    assert_eq!(info.map(|i| i.free), Some(7), "but coming back answers them at once");
}

// A fake statfs that records every path it was asked for and fails for one named "gone".
fn recording() -> (FsInfo, Receiver<Event>, Arc<std::sync::Mutex<Vec<PathBuf>>>) {
    let (events, rx) = crate::backend::events::channel();
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::clone(&asked);
    let reader: Reader = Arc::new(move |path: PathBuf| {
        seen.lock().unwrap().push(path.clone());
        if path.ends_with("gone") { (None, None) } else { (figures(7), Some(0x65735546)) }
    });
    (gui(FsInfo::with_reader(events, reader)), rx, asked)
}

#[test]
fn a_list_of_a_missing_folder_never_blanks_the_folder_on_screen() {
    let (mut fs, rx, _) = recording();
    let photos = share_dir("photos");
    fs.list_arrived(&photos, &Listing::new());
    let _ = fs.finish(next_done(&rx), &photos);
    fs.list_arrived(&share_dir("gone"), &Listing::new());
    assert!(fs.finish(next_done(&rx), &photos).is_none(), "a failed statfs of another folder prints nothing for photos");
    assert_eq!(fs.answer(&photos).0.map(|i| i.free), Some(7), "and photos keeps its figures");
}

#[test]
fn a_failed_list_during_a_busy_statfs_still_reads_the_folder_on_screen() {
    let (mut fs, rx, _) = counted(200, 7);
    let photos = share_dir("photos");
    fs.list_arrived(&share_dir("w"), &Listing::new());
    fs.list_arrived(&photos, &Listing::new());
    fs.list_arrived(&share_dir("gone"), &Listing::new());
    assert!(fs.finish(next_done(&rx), &photos).is_none(), "w's figures never print over photos");
    let second = next_done(&rx);
    assert_eq!(second.dir, photos, "the folder on screen is read, not the failed list's folder");
}

#[test]
fn a_folder_left_behind_a_hung_statfs_is_never_read() {
    let (mut fs, rx, calls) = counted(0, 7);
    let root = PathBuf::from(SHARE);
    fs.inflight.insert(root.clone(), (Instant::now(), 90));
    fs.list_arrived(&share_dir("b"), &Listing::new());
    fs.inflight.insert(root.clone(), (Instant::now().checked_sub(FSINFO_DEADLINE + Duration::from_secs(1)).unwrap(), 90));
    fs.list_arrived(&share_dir("c"), &Listing::new());
    let done = next_done(&rx);
    assert_eq!(done.dir, share_dir("c"));
    let _ = fs.finish(done, &share_dir("c"));
    let _ = fs.finish(Done { root: root.clone(), dir: share_dir("a"), seq: 90, info: figures(7), tagable: true }, &share_dir("c"));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "b, left behind the hung statfs, was read anyway");
}

#[test]
fn a_hung_statfs_landing_late_never_frees_its_replacement_slot() {
    let (mut fs, rx, calls) = counted(300, 7);
    let root = PathBuf::from(SHARE);
    fs.inflight.insert(root.clone(), (Instant::now().checked_sub(FSINFO_DEADLINE + Duration::from_secs(1)).unwrap(), 90));
    fs.list_arrived(&share_dir("a"), &Listing::new());
    let _ = fs.finish(Done { root: root.clone(), dir: share_dir("a"), seq: 90, info: figures(7), tagable: true }, &share_dir("a"));
    fs.list_arrived(&share_dir("b"), &Listing::new());
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the hung statfs's late landing let a second one start beside its replacement");
    let _ = next_done(&rx);
}

#[test]
fn a_phone_storage_is_read_where_it_is_listed_not_at_the_device() {
    let (mut fs, rx, asked) = recording();
    let storage = PathBuf::from("/run/user/1000/gvfs/mtp:host=Phone/Internal shared storage/DCIM");
    fs.list_arrived(&storage, &Listing::new());
    let done = next_done(&rx);
    assert_eq!(done.root, Path::new("/run/user/1000/gvfs/mtp:host=Phone"), "keyed by the device");
    assert_eq!(*asked.lock().unwrap(), vec![storage], "read at the storage folder, whose figures the device root does not carry");
}

#[test]
fn a_hung_statfs_past_the_deadline_lets_the_next_ask_try_again() {
    let (mut fs, _rx, calls) = counted(0, 7);
    fs.inflight.insert(PathBuf::from(SHARE), (Instant::now().checked_sub(FSINFO_DEADLINE + Duration::from_secs(1)).unwrap(), 90));
    fs.list_arrived(&share_dir("a"), &Listing::new());
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "a presumed hung statfs does not hold the share's figures forever");
}

#[test]
fn a_kernel_network_mount_is_answered_off_the_loop() {
    let (mut fs, rx, _) = counted(300, 9);
    // Sample body: mount point, then "-" and the fstype, the fields answer_in reads.
    let cifs = "31 23 0:27 / /media/nas rw - cifs //nas/media rw\n";
    let t = Instant::now();
    let (info, class, _) = fs.answer_in(Path::new("/media/nas/photos"), cifs);
    assert!(info.is_none() && t.elapsed() < Duration::from_millis(250), "cifs pays its round trip on the worker");
    assert_eq!(class, "network");
    let done = next_done(&rx);
    assert_eq!((done.root.as_path(), done.dir.as_path()), (Path::new("/media/nas"), Path::new("/media/nas/photos")), "keyed by the mount point, read at the folder");
}

#[test]
fn a_local_directory_still_answers_synchronously() {
    let (events, _rx) = crate::backend::events::channel();
    let mut fs = FsInfo::new(events);
    let sandbox = super::super::testdir::TestDir::new("fsinfoearly-local");
    let (info, class, _) = fs.answer(sandbox.path());
    assert!(info.is_some_and(|i| i.free > 0), "a local statfs answers its figures at once");
    assert_eq!(class, super::super::extclass::classify(sandbox.path()), "and its class is the full one");
    assert!(fs.inflight.is_empty(), "no worker for a local directory");
}

#[test]
fn a_symlink_into_a_network_mount_answers_off_the_loop() {
    let sandbox = super::super::testdir::TestDir::new("fsinfolink");
    let target = sandbox.path().join("real");
    std::fs::create_dir(&target).unwrap();
    let link = sandbox.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let body = format!("1 0 0:30 / / rw - ext4 /dev/a rw\n30 1 0:9 / {} rw - cifs //nas/media rw\n", target.display());
    // A statfs that takes half a second, so an answer inside a quarter of one proves it ran on the worker.
    let (mut fs, rx, _) = counted(500, 7);
    super::super::extclass::test_reset_link_reads();
    let t = Instant::now();
    let (info, class, _) = fs.answer_in(&link, &body);
    assert!(t.elapsed() < Duration::from_millis(250), "no statfs ran on the loop, took {:?}", t.elapsed());
    let reads = super::super::extclass::test_link_reads();
    assert!(reads.contains(&link) && reads.iter().all(|p| !p.starts_with(&target)), "the link is read and nothing inside the share: {:?}", reads);
    assert_eq!(class, "network", "a symlink into a cifs mount is network, not local");
    assert!(info.is_none(), "its figures arrive on the worker, never blocked");
    assert_eq!(next_done(&rx).info.map(|i| i.free), Some(7), "and the worker does report them");
}

#[test]
fn a_directory_is_a_system_volume_by_the_mount_owning_its_resolved_path() {
    let sandbox = super::super::testdir::TestDir::new("fsinfosystem");
    let (real, link) = (sandbox.path().join("real"), sandbox.path().join("link"));
    std::fs::create_dir(&real).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let (mut fs, _rx, _) = counted(0, 7);
    let data = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 8:17 / {} rw - ext4 /dev/b rw\n", real.display());
    assert!(!fs.answer_in(&link, &data).2, "a link into a data volume is no system volume, whatever volume the link sits on");
    let root = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 8:17 / {} rw - ext4 /dev/b rw\n31 30 8:1 /sub {} rw - ext4 /dev/a rw\n", sandbox.path().display(), real.display());
    assert!(fs.answer_in(&link, &root).2, "the same link into the device holding / is");
}

#[test]
fn a_btrfs_subvolume_is_a_system_volume_by_the_number_the_mount_table_names() {
    use super::super::mountinfo::dev_majmin;
    use std::os::unix::fs::MetadataExt;
    let sandbox = super::super::testdir::TestDir::new("fsinfobtrfs");
    let other = sandbox.path().join("other");
    std::fs::create_dir(&other).unwrap();
    // On btrfs stat gives a subvolume its own anonymous number while every mount line of the filesystem names one, never that number.
    let stat = dev_majmin(std::fs::metadata(sandbox.path()).unwrap().dev());
    let shared = if stat == "0:29" { "0:30" } else { "0:29" };
    let body = format!("1 0 {shared} /@ / rw - btrfs /dev/mapper/root rw\n2 1 {shared} /@sub {} rw - btrfs /dev/mapper/root rw\n3 2 0:52 / {} rw - btrfs /dev/sdc1 rw\n", sandbox.path().display(), other.display());
    let (mut fs, _rx, _) = counted(0, 7);
    assert!(fs.answer_in(sandbox.path(), &body).2, "a subvolume of the filesystem holding / is on the system volume");
    assert!(!fs.answer_in(&other, &body).2, "a second btrfs filesystem is judged alone");
}

#[test]
fn a_raw_kernel_share_is_decided_before_any_resolve() {
    let sandbox = super::super::testdir::TestDir::new("fsinforawfirst");
    let share = sandbox.path().join("share");
    let real = sandbox.path().join("real");
    std::fs::create_dir(&share).unwrap();
    std::fs::create_dir(&real).unwrap();
    let link = share.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let body = format!("1 0 0:30 / / rw - ext4 /dev/a rw\n30 1 0:9 / {} rw - cifs //nas/media rw\n", share.display());
    let (mut fs, _rx, _) = counted(0, 7);
    let (_, class, _) = fs.answer_in(&link, &body);
    assert_eq!(class, "network", "a path under a cifs mount is network by its own mount, before its link is followed");
}

#[test]
fn a_decided_path_takes_no_statfs_and_a_local_one_takes_exactly_one() {
    use super::super::fsinfo::{statfs_calls, test_reset_statfs};
    let cifs = "31 23 0:27 / /media/nas rw - cifs //nas/media rw\n";
    let (mut fs, _rx, _) = counted(0, 7);
    test_reset_statfs();
    let (_, class, _) = fs.answer_in(Path::new("/media/nas/photos"), cifs);
    assert_eq!(class, "network");
    assert_eq!(statfs_calls(), 0, "fstype already decided; a dead server gets no statfs for nothing");
    let sandbox = super::super::testdir::TestDir::new("fsinfoonestatfs");
    let local = "1 0 0:30 / / rw - ext4 /dev/a rw\n";
    let (events, _rx) = crate::backend::events::channel();
    let mut real = FsInfo::new(events);
    test_reset_statfs();
    let (info, class, _) = real.answer_in(sandbox.path(), local);
    assert!(info.is_some_and(|i| i.free > 0), "a local statfs answers its figures at once");
    assert_eq!(class, "");
    assert_eq!(statfs_calls(), 1, "figures and magic share one statfs, not two");
}

// A fake statfs that records every path it was asked for, refusing the ones named in `refuse`.
fn recorded(refuse: &'static [&'static str]) -> (FsInfo, Receiver<Event>, Arc<std::sync::Mutex<Vec<PathBuf>>>) {
    let (events, rx) = crate::backend::events::channel();
    let paths = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::clone(&paths);
    let reader: Reader = Arc::new(move |path: PathBuf| {
        let refused = refuse.iter().any(|name| path.ends_with(name));
        seen.lock().unwrap().push(path);
        if refused { (None, None) } else { (figures(7), Some(0x65735546)) }
    });
    (gui(FsInfo::with_reader(events, reader)), rx, paths)
}

#[test]
fn a_share_statfs_opens_the_listed_file_and_never_the_folder() {
    let (mut fs, rx, paths) = recorded(&[]);
    let dir = share_dir("ten-thousand");
    fs.list_arrived(&dir, &listed(&[("sub", true), ("a.txt", false)]));
    let done = next_done(&rx);
    assert_eq!(*paths.lock().unwrap(), vec![dir.join("a.txt")], "one statfs, on the listing's first plain file");
    assert_eq!(done.dir, dir, "the figures still belong to the folder");
    assert_eq!(fs.finish(done, &dir).flatten().map(|i| i.free), Some(7));
}

#[test]
fn a_file_gone_since_the_listing_falls_back_to_the_folder() {
    let (mut fs, rx, paths) = recorded(&["a.txt"]);
    let dir = share_dir("photos");
    fs.list_arrived(&dir, &listed(&[("a.txt", false)]));
    let done = next_done(&rx);
    assert_eq!(*paths.lock().unwrap(), vec![dir.join("a.txt"), dir.clone()], "the refused file, then the folder");
    assert_eq!(fs.finish(done, &dir).flatten().map(|i| i.free), Some(7), "the folder's own figures print");
}

#[test]
fn a_file_from_another_folder_or_a_local_folder_is_never_statted() {
    let (mut fs, rx, paths) = recorded(&[]);
    let (a, b) = (share_dir("a"), share_dir("b"));
    fs.list_arrived(&a, &listed(&[("x.txt", false)]));
    let done = next_done(&rx);
    let _ = fs.finish(done, &a);
    fs.refresh(PathBuf::from(SHARE), b.clone());
    let _ = next_done(&rx);
    let local = PathBuf::from("/home/someone/docs");
    fs.list_arrived(&local, &listed(&[("y.txt", false)]));
    fs.refresh(local.clone(), local.clone());
    let _ = next_done(&rx);
    assert_eq!(*paths.lock().unwrap(), vec![a.join("x.txt"), b, local], "only a share's own listed file stands in for it");
}

#[test]
fn a_folder_of_folders_statfs_reads_the_folder() {
    let (mut fs, rx, paths) = recorded(&[]);
    let dir = share_dir("albums");
    fs.list_arrived(&dir, &listed(&[("2024", true), ("2025", true)]));
    let _ = next_done(&rx);
    assert_eq!(*paths.lock().unwrap(), vec![dir], "no file to open, so the folder itself");
}

fn listed(rows: &[(&str, bool)]) -> Listing {
    let mut listing = Listing::new();
    for (name, is_dir) in rows {
        listing.push(name, *is_dir);
    }
    listing
}
