use super::*;
use crate::backend::testdir::TestDir;
use std::time::Duration;

// Bounds a wait for the follower's answer, so a missing one fails instead of hanging.
const DEADLINE: Duration = Duration::from_secs(10);
// How long silence must last before a test believes the follower said nothing more.
const QUIET: Duration = Duration::from_millis(600);

// Each backend process owns one follower; parallel test cases must not share its queue or watches.
fn in_own_process(name: &str) -> bool {
    const CHILD: &str = "FLEA_ANCESTRY_TEST";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        return false;
    }
    let full_name = format!("backend::watch::ancestry::tests::{name}");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &full_name, "--nocapture"])
        .env(CHILD, name)
        .output()
        .unwrap();
    assert!(
        output.status.success()
            && String::from_utf8_lossy(&output.stdout).contains("test result: ok. 1 passed;"),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

#[test]
fn an_unused_client_starts_no_follower() {
    if in_own_process("an_unused_client_starts_no_follower") {
        return;
    }
    let threads = || std::fs::read_dir("/proc/self/task").unwrap().count();
    let before = threads();
    let (tx, _rx) = crate::backend::events::channel();
    let mut ancestry = Ancestry::new(tx);
    ancestry.check();
    ancestry.forget();
    drop(ancestry);
    assert_eq!(
        threads(),
        before,
        "unused cleanup starts no follower threads"
    );
}

#[test]
fn an_overflow_still_reports_folders_that_moved_in_the_lost_events() {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    let dir = TestDir::new("ancestry-overflow");
    let (one, two, noise) = (dir.dir("p/one"), dir.dir("q/two"), dir.dir("noise"));
    let fd = unsafe { inotify_init1(IN_CLOEXEC | crate::backend::watch::IN_NONBLOCK) };
    assert!(fd >= 0, "this box has no inotify to test with");
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let (events, replies) = crate::backend::events::channel();
    let mut follower = Follower {
        fd,
        clients: HashMap::new(),
        refs: HashMap::new(),
    };
    follower.follow(1, events.clone(), one.clone());
    follower.follow(2, events, two.clone());
    let limit: usize = std::fs::read_to_string("/proc/sys/fs/inotify/max_queued_events")
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let noise = CString::new(noise.as_os_str().as_encoded_bytes()).unwrap();
    for _ in 0..=limit {
        let wd = unsafe { inotify_add_watch(fd, noise.as_ptr(), MASK) };
        assert!(wd >= 0);
        assert_eq!(unsafe { inotify_rm_watch(fd, wd) }, 0);
    }
    std::fs::rename(dir.join("p"), dir.join("moved-p")).unwrap();
    std::fs::rename(dir.join("q"), dir.join("moved-q")).unwrap();
    let mut queued = Vec::new();
    let mut buf = [0; BUF];
    loop {
        let n = unsafe { read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            assert_eq!(io::Error::last_os_error().kind(), io::ErrorKind::WouldBlock);
            break;
        }
        assert!(n > 0);
        queued.extend_from_slice(&buf[..n as usize]);
    }
    let (mut overflow, mut moved) = (false, false);
    each_event(&queued, |_, mask| {
        overflow |= mask & crate::backend::watch::IN_Q_OVERFLOW != 0;
        moved |= mask & IN_MOVE_SELF != 0;
    });
    assert!(
        overflow && !moved,
        "the kernel dropped both moves behind the full queue"
    );
    // Replay complete kernel records through an EOF-bounded reader using the real pump.
    std::fs::write(dir.join("queued"), queued).unwrap();
    let reader = std::fs::File::open(dir.join("queued")).unwrap();
    let (asks, queue) = mpsc::channel();
    pump(reader.as_raw_fd(), asks);
    follower.run(queue);
    let mut gone: Vec<_> = replies
        .try_iter()
        .map(|event| match event {
            Event::Gone(gone) => gone,
            _ => panic!("a lost move must answer gone"),
        })
        .collect();
    gone.sort_by(|a, b| a.path.cmp(&b.path));
    assert_eq!(
        gone,
        [
            Gone {
                path: one,
                parent: dir.path().to_path_buf()
            },
            Gone {
                path: two,
                parent: dir.path().to_path_buf()
            },
        ]
    );
    drop(owned);
}

#[test]
fn a_followed_folder_is_the_same_until_it_moves_or_goes() {
    let dir = TestDir::new("ancestry-verdict");
    let open = dir.dir("p/open-me");
    let recorded = identity(&open);
    assert!(recorded.is_some(), "a folder has an identity");
    assert_eq!(verdict(&open, recorded), Verdict::Same);
    // The old folder stays under another name, so the new one cannot take its inode.
    std::fs::rename(&open, dir.join("p/old")).expect("the sandbox renames its own folder");
    std::fs::create_dir(&open).expect("the sandbox makes a folder under the old name");
    assert_eq!(
        verdict(&open, recorded),
        Verdict::Replaced,
        "another folder under the same name re-lists"
    );
    std::fs::rename(dir.join("p"), dir.join("q")).expect("the sandbox moves the folder above");
    assert_eq!(
        verdict(&open, recorded),
        Verdict::Gone(dir.path().to_path_buf()),
        "a moved folder above lands on the nearest folder still there"
    );
    std::fs::write(dir.join("p"), "a file where the folder was")
        .expect("the sandbox writes a file");
    assert_eq!(
        verdict(&open, recorded),
        Verdict::Gone(dir.path().to_path_buf()),
        "a file is no folder to land on"
    );
}

// A folder that listed but never had an identity is never called gone by the follower; its re-list decides.
#[test]
fn a_folder_with_no_recorded_identity_is_never_called_gone() {
    let dir = TestDir::new("ancestry-unstatable");
    let never = dir.join("p/never-statted");
    assert_eq!(identity(&never), None);
    assert_eq!(verdict(&never, None), Verdict::Same);
    let now = dir.dir("p/never-statted");
    assert_eq!(
        verdict(&now, None),
        Verdict::Replaced,
        "a folder that becomes statable is followed anew"
    );
}

#[test]
fn the_open_folder_is_owed_once_then_only_the_folder_it_was_sent_to() {
    let mut ancestry = Ancestry::default();
    let (base, parent) = (Path::new("/w/p/o"), Path::new("/w/p"));
    assert!(ancestry.owes(base, base));
    assert!(
        !ancestry.owes(parent, base),
        "a folder above is not the open one"
    );
    let gone = Gone {
        path: base.to_path_buf(),
        parent: parent.to_path_buf(),
    };
    ancestry.left_for(&gone);
    assert!(!ancestry.owes(base, base), "the open folder was said once");
    assert!(
        ancestry.owes(parent, base),
        "the folder the pane was sent to failing too is said in turn"
    );
    assert_eq!(
        ancestry.crossed(base),
        Some(&gone),
        "a re-list of the gone folder gets its line again"
    );
    assert_eq!(ancestry.crossed(parent), None);
    ancestry.follow(parent);
    assert!(
        ancestry.owes(parent, parent),
        "a landed listing starts over"
    );
    assert_eq!(ancestry.crossed(base), None, "and forgets the gone line");
}

fn gone_from(rx: &std::sync::mpsc::Receiver<Event>) -> Gone {
    loop {
        match rx.recv_timeout(DEADLINE).expect("the follower answers") {
            Event::Gone(gone) => return gone,
            _ => continue,
        }
    }
}

// S3-7: a folder above the open one moving away left the pane on a path that no longer exists.
#[test]
fn a_folder_above_moving_away_names_the_nearest_folder_still_there() {
    if in_own_process("a_folder_above_moving_away_names_the_nearest_folder_still_there") {
        return;
    }
    let dir = TestDir::new("ancestry-above");
    let open = dir.dir("p/mid/open-me");
    let (tx, rx) = crate::backend::events::channel();
    let mut ancestry = Ancestry::new(tx);
    ancestry.follow(&open);
    // The follower arms on its own thread; a check it answers proves it holds the watches.
    ancestry.check();
    assert!(
        matches!(rx.recv_timeout(DEADLINE), Ok(Event::PollChanged(path)) if path == open),
        "a check on a folder still there re-lists it"
    );
    std::fs::rename(dir.join("p"), dir.join("elsewhere"))
        .expect("the sandbox moves the folder above");
    assert_eq!(
        gone_from(&rx),
        Gone {
            path: open.clone(),
            parent: dir.path().to_path_buf()
        }
    );
    std::fs::rename(dir.join("elsewhere"), dir.join("p")).expect("the sandbox moves it back");
    std::fs::rename(dir.join("p"), dir.join("again")).expect("the sandbox moves it again");
    assert!(
        rx.recv_timeout(QUIET).is_err(),
        "a gone folder is said once until a listing lands again"
    );
}

// The open folder deleted before the follower first stats it: the listing watch's check still re-lists, and that re-list answers gone.
#[test]
fn a_folder_gone_before_its_first_stat_is_re_listed_on_check() {
    let dir = TestDir::new("ancestry-early");
    let open = dir.dir("p/open-me");
    std::fs::remove_dir(&open).expect("the sandbox deletes its own empty folder");
    let (tx, rx) = crate::backend::events::channel();
    let mut ancestry = Ancestry::new(tx);
    ancestry.follow(&open);
    ancestry.check();
    assert!(
        matches!(rx.recv_timeout(DEADLINE), Ok(Event::PollChanged(path)) if path == open),
        "the check re-lists the open folder"
    );
    assert!(
        rx.recv_timeout(QUIET).is_err(),
        "and the follower says nothing else"
    );
}

// Each folder's watch is counted per client, and the last client leaving takes it out of the kernel.
#[test]
fn a_shared_folder_watch_goes_when_the_last_client_leaves() {
    use std::os::fd::FromRawFd;
    let dir = TestDir::new("ancestry-refs");
    let (one, two) = (dir.dir("p/one"), dir.dir("p/two"));
    let fd = unsafe { inotify_init1(IN_CLOEXEC) };
    assert!(fd >= 0, "this box has no inotify to test with");
    let mut follower = Follower {
        fd,
        clients: HashMap::new(),
        refs: HashMap::new(),
    };
    let (tx, _rx) = crate::backend::events::channel();
    follower.follow(1, tx.clone(), one);
    follower.follow(2, tx, two);
    let shared: Vec<c_int> = follower.clients[&1]
        .wds
        .iter()
        .copied()
        .filter(|wd| follower.clients[&2].wds.contains(wd))
        .collect();
    assert!(
        !shared.is_empty(),
        "the two clients share the folders above them"
    );
    follower.leave(1);
    assert!(
        shared.iter().all(|wd| follower.refs.get(wd) == Some(&1)),
        "the shared watches stay for the second client"
    );
    let held: Vec<c_int> = follower.refs.keys().copied().collect();
    follower.leave(2);
    assert!(
        follower.refs.is_empty(),
        "nothing is counted once neither follows"
    );
    assert!(
        held.iter()
            .all(|&wd| unsafe { inotify_rm_watch(fd, wd) } < 0),
        "and the kernel holds none of them"
    );
    drop(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) });
}

#[test]
fn two_clients_under_one_folder_keep_its_watch_until_neither_follows() {
    if in_own_process("two_clients_under_one_folder_keep_its_watch_until_neither_follows") {
        return;
    }
    let dir = TestDir::new("ancestry-shared");
    let (one, two) = (dir.dir("p/one"), dir.dir("p/two"));
    let (tx_one, rx_one) = crate::backend::events::channel();
    let (tx_two, rx_two) = crate::backend::events::channel();
    let (mut first, mut second) = (Ancestry::new(tx_one), Ancestry::new(tx_two));
    first.follow(&one);
    second.follow(&two);
    first.forget();
    second.check();
    assert!(
        matches!(rx_two.recv_timeout(DEADLINE), Ok(Event::PollChanged(_))),
        "the second client is armed"
    );
    std::fs::rename(dir.join("p"), dir.join("q")).expect("the sandbox moves the shared folder");
    assert_eq!(
        gone_from(&rx_two),
        Gone {
            path: two,
            parent: dir.path().to_path_buf()
        },
        "one client leaving keeps the shared watch"
    );
    assert!(
        rx_one.recv_timeout(QUIET).is_err(),
        "a client that forgot is told nothing"
    );
}
