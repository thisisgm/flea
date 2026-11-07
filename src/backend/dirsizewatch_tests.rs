use super::*;
use crate::backend::dirsizereq::{queue_dirsizes, report_done, start_next};
use crate::backend::events::{channel, Event};
use crate::backend::iomount::ListOut;
use crate::backend::listing::Listing;
use crate::backend::run::adopt_listed;
use crate::backend::state::{State, Tables};
use crate::backend::testdir::TestDir;
use crate::backend::thumbs::Pool;
use std::io::Write;
use std::sync::mpsc::Receiver;
use std::time::Duration;

const IN_OPEN: u32 = 0x20;
const IN_NONBLOCK: c_int = 0x800;

// The reporter's fixture: heavy/ holding 30 folders of 100 one-byte files.
fn heavy(d: &TestDir) -> PathBuf {
    let heavy = d.dir("fx/heavy");
    for n in 1..=30 {
        let dir = heavy.join(format!("dir-{n}"));
        std::fs::create_dir(&dir).unwrap();
        for i in 1..=100 {
            std::fs::write(dir.join(format!("file-{i}")), "x").unwrap();
        }
    }
    heavy
}

// The reporter's count: inotify IN_OPEN on heavy/ and each folder below it, read without blocking.
struct Opens(OwnedFd);

impl Opens {
    fn watch(heavy: &Path) -> Opens {
        let fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
        assert!(fd >= 0, "inotify is unavailable");
        let mut dirs = vec![heavy.to_path_buf()];
        dirs.extend((1..=30).map(|n| heavy.join(format!("dir-{n}"))));
        for dir in dirs {
            let path = CString::new(dir.as_os_str().as_encoded_bytes()).unwrap();
            assert!(unsafe { inotify_add_watch(fd, path.as_ptr(), IN_OPEN | IN_ONLYDIR) } >= 0);
        }
        Opens(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    // Distinct folders opened since the last call, as the report counts them.
    fn take(&self) -> usize {
        let mut buf = [0u8; BUF];
        let mut opened = std::collections::HashSet::new();
        loop {
            let n = unsafe { read(self.0.as_raw_fd(), buf.as_mut_ptr() as *mut c_void, BUF) };
            if n <= 0 {
                return opened.len();
            }
            each_event(&buf[..n as usize], |wd, _| {
                opened.insert(wd);
            });
        }
    }
}

struct Rig {
    st: State,
    tb: Tables,
    pool: Pool,
    events: Receiver<Event>,
}

impl Rig {
    fn new(d: &TestDir) -> Rig {
        let tb = Tables::load();
        let (events, rx) = channel();
        let (results, _) = std::sync::mpsc::channel();
        let pool = Pool::new(
            1,
            results,
            d.join("cache"),
            Arc::clone(&tb.aliases),
            Arc::clone(&tb.thumbs),
        );
        Rig {
            st: State::new(crate::backend::dirsizeworker::Worker::new(events)),
            tb,
            pool,
            events: rx,
        }
    }

    // A tab's list landing on `base`, which holds the one folder `name`, naming the other tabs' folders.
    fn list(&mut self, base: &Path, name: &str, keep: &[&Path]) {
        self.st
            .settled
            .keep(keep.iter().map(|path| path.to_path_buf()).collect());
        let mut listing = Listing::new();
        listing.push(name, true);
        let (first_metas, first_ms) = crate::backend::meta::stat_range(base, &listing, 0, 1);
        let done = ListOut {
            listing,
            read_ms: 0.,
            sort_ms: 0.,
            sized: Vec::new(),
            dev: 0,
            writable: true,
            first_metas,
            first_start: 0,
            first_ms,
            watch_wd: -1,
            watch_mount: PathBuf::new(),
        };
        let mut out = Vec::new();
        adopt_listed(
            &mut out,
            &mut self.st,
            &self.pool,
            &self.tb,
            base.to_str().unwrap(),
            done,
            false,
        );
    }

    // The client's dirsize for row 0, answered at once from a cache or by the worker: its bytes and whether it walked.
    fn size(&mut self) -> (u64, bool) {
        let mut out = Vec::new();
        queue_dirsizes(&mut out, &mut self.st, &[0]);
        let walked = out.is_empty();
        if walked {
            start_next(&mut self.st);
            let Ok(Event::DirSize(done)) = self.events.recv_timeout(Duration::from_secs(30)) else {
                panic!("no walk landed")
            };
            report_done(&mut out, &mut self.st, done);
        }
        let line = String::from_utf8(out).unwrap();
        let document = crate::jsondoc::parse(line.trim()).unwrap();
        assert_eq!(
            document
                .get("partial")
                .and_then(crate::jsondoc::Json::as_bool),
            Some(false),
            "{line}"
        );
        let Some(crate::jsondoc::Json::Num(bytes)) = document.get("bytes") else {
            panic!("{line}")
        };
        (bytes.parse().unwrap(), walked)
    }
}

#[test]
fn a_tab_switch_back_re_walks_nothing_and_a_deep_write_is_counted() {
    let d = TestDir::new("dirsize-tabs");
    let heavy = heavy(&d);
    let fx = d.join("fx");
    let other = d.dir("other");
    std::fs::create_dir(other.join("folder")).unwrap();
    let mut rig = Rig::new(&d);
    rig.list(&fx, "heavy", &[]);
    let (bytes, walked) = rig.size();
    assert!(walked, "the first ask walks");
    let opens = Opens::watch(&heavy);
    // Tab 2 opens on the same folder, then 1 and 2 trade places as in the report.
    for (base, name, keep) in [
        (&fx, "heavy", &fx),
        (&other, "folder", &fx),
        (&fx, "heavy", &other),
    ] {
        rig.list(base, name, &[keep]);
        let (_, walked) = rig.size();
        assert_eq!(
            walked,
            base == &other,
            "only the other folder's first ask walks"
        );
    }
    assert_eq!(rig.size(), (bytes, false), "heavy answers its settled size");
    assert_eq!(
        opens.take(),
        0,
        "a switch back opens none of the 31 folders"
    );

    // Negative control: a listing no tab keeps gives heavy's size back, so the next return walks all 31 again.
    rig.list(&other, "folder", &[]);
    rig.list(&fx, "heavy", &[&other]);
    assert_eq!(rig.size(), (bytes, true));
    assert_eq!(
        opens.take(),
        31,
        "the counter sees all 31 folders a walk reads"
    );

    // A write three levels down changes no folder's mtime above it, and still retires the settled size.
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(heavy.join("dir-7/file-3"))
        .unwrap();
    file.write_all(b"12345").unwrap();
    drop(file);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while rig.st.settled.get(&heavy).is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "the write never reached the size watch"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    rig.list(&fx, "heavy", &[&other]);
    assert_eq!(
        rig.size(),
        (bytes + 5, true),
        "the next ask walks and counts the write"
    );
}

#[test]
fn a_size_is_kept_only_with_every_folder_watched_and_unchanged() {
    let d = TestDir::new("dirsize-pins");
    let tree = d.dir("tree");
    std::fs::create_dir(tree.join("inner")).unwrap();
    let watch = TreeWatch::new();
    let never = || false;
    let deadline = Instant::now() + Duration::from_secs(30);
    let (size, pins) = walk_watched(&tree, deadline, &never, &watch);
    assert!(
        !size.partial && pins.fresh() && pins.wds.len() == 2,
        "a whole walk pins both folders"
    );
    let (_, again) = walk_watched(&tree, deadline, &never, &watch);
    assert_eq!(
        lock(&watch.0).folders.len(),
        2,
        "a second walk shares the same two watches"
    );
    drop(again);
    assert!(
        pins.fresh(),
        "another walk letting go keeps the first one's watches"
    );
    let (_, cancelled) = walk_watched(&tree, deadline, &|| true, &watch);
    assert!(!cancelled.fresh(), "a cancelled walk pins nothing");
    let file = tree.join("file");
    std::fs::write(&file, "x").unwrap();
    while pins.fresh() {
        assert!(
            Instant::now() < deadline,
            "the create never reached the size watch"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(pins);
    assert!(
        lock(&watch.0).folders.is_empty(),
        "the last pin removes every watch"
    );
    let mut settled = Settled::default();
    let (_, pins) = walk_watched(&file, deadline, &never, &watch);
    settled.settle(file.clone(), 1, pins);
    assert_eq!(
        settled.len(),
        0,
        "a walk that read no folder of its own is never kept"
    );
}

#[test]
fn a_full_watch_budget_keeps_nothing() {
    let d = TestDir::new("dirsize-cap");
    let tree = d.dir("tree");
    let watch = TreeWatch::new();
    for wd in 0..CAP as c_int {
        lock(&watch.0).folders.insert(
            -1000 - wd,
            Folder {
                pins: 1,
                changed: false,
            },
        );
    }
    let (size, pins) = walk_watched(
        &tree,
        Instant::now() + Duration::from_secs(30),
        &|| false,
        &watch,
    );
    assert!(!size.partial, "the size itself is still whole");
    assert!(
        lock(&watch.0).fd >= 0,
        "inotify opened, so only the budget refused"
    );
    assert!(!pins.fresh(), "but a tree past the budget is not kept");
    lock(&watch.0).folders.clear();
}

#[test]
fn a_path_that_comes_to_name_another_folder_is_walked_again() {
    let d = TestDir::new("dirsize-moved");
    let src = d.dir("project").join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("file"), "x").unwrap();
    let watch = TreeWatch::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    let (size, pins) = walk_watched(&src, deadline, &|| false, &watch);
    let mut settled = Settled::default();
    settled.settle(src.clone(), size.bytes, pins);
    assert_eq!(
        settled.get(&src),
        Some(size.bytes),
        "an unchanged folder answers"
    );
    // Moving an ancestor fires nothing on the watched folders below it.
    std::fs::rename(d.join("project"), d.join("project.old")).unwrap();
    std::fs::create_dir_all(&src).unwrap();
    assert_eq!(
        settled.get(&src),
        None,
        "the new folder at that path is walked again"
    );
}
