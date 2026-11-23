//! The wake thread's contract: no tick while every folder is watched, a late folder through its parent
//! and then without it, a repointed link, the tick while inotify cannot watch, and a drop ends it.
use super::{start, start_with};
use std::{sync::mpsc, time::Duration};

// A tick no test run reaches, so only an inotify event can wake.
const UNREACHED_TICK: Duration = Duration::from_secs(3600);
const GUARD: Duration = Duration::from_secs(10);
// Long enough for a stray tick or event to land if one were coming.
const QUIET: Duration = Duration::from_millis(300);

// A fresh 0700 folder under an unguessable name. mkdir fails on a path planted in its place, never
// follows it, and nothing else can enter the folder once it exists.
pub(crate) fn scratch(name: &str) -> std::path::PathBuf {
    use std::os::unix::fs::DirBuilderExt;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "flea-dirwake-{name}-{}-{nanos:08x}-{next}",
        std::process::id()
    ));
    std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    dir
}

#[test]
fn a_watched_folder_sleeps_until_a_file_in_it_changes() {
    let dir = scratch("sleeps");
    let (send, woken) = mpsc::channel();
    let _wake = start(
        "test",
        vec![dir.clone()],
        Duration::from_millis(10),
        move || send.send(()).is_ok(),
    );
    // A 10 ms tick would have fired many times by now if a fully watched folder still ticked.
    assert!(
        woken.recv_timeout(QUIET).is_err(),
        "a fully watched folder must not tick"
    );
    std::fs::write(dir.join("ui.json"), "{}").unwrap();
    assert!(
        woken.recv_timeout(GUARD).is_ok(),
        "a write in the folder must wake"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_folder_created_later_is_seen_through_its_parent_and_then_watched() {
    let root = scratch("later");
    let dir = root.join("state/flea");
    let (send, woken) = mpsc::channel();
    let _wake = start("test", vec![dir.clone()], UNREACHED_TICK, move || {
        send.send(()).is_ok()
    });
    std::fs::create_dir_all(&dir).unwrap();
    assert!(
        woken.recv_timeout(GUARD).is_ok(),
        "creating the folder must wake"
    );
    // Each level of the new path may wake once; drain those before the write that matters.
    while woken.recv_timeout(QUIET).is_ok() {}
    std::fs::write(dir.join("ui.json"), "{}").unwrap();
    assert!(
        woken.recv_timeout(GUARD).is_ok(),
        "a write in the new folder must wake once it is armed"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn dropping_the_handle_ends_the_thread() {
    let dir = scratch("drop");
    let (send, woken) = mpsc::channel::<()>();
    let wake = start("test", vec![dir.clone()], UNREACHED_TICK, move || {
        send.send(()).is_ok()
    });
    drop(wake);
    // The thread owned the only sender; it ends, so the channel disconnects instead of timing out.
    assert!(matches!(
        woken.recv_timeout(GUARD),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

// Every wake already sent is taken first; the thread then ends, so the channel disconnects.
fn ends(woken: &mpsc::Receiver<()>) -> bool {
    loop {
        match woken.recv_timeout(GUARD) {
            Ok(()) => {}
            Err(error) => return error == mpsc::RecvTimeoutError::Disconnected,
        }
    }
}

#[test]
fn the_parent_watched_for_a_missing_folder_is_dropped_once_the_folder_exists() {
    let root = scratch("unwatch");
    let dir = root.join("state/flea");
    let (send, woken) = mpsc::channel();
    let _wake = start("test", vec![dir.clone()], UNREACHED_TICK, move || {
        send.send(()).is_ok()
    });
    std::fs::create_dir_all(&dir).unwrap();
    assert!(
        woken.recv_timeout(GUARD).is_ok(),
        "creating the folder must wake"
    );
    while woken.recv_timeout(QUIET).is_ok() {}
    // The root was watched while the folder was missing; another app's file there must not wake now.
    std::fs::write(root.join("other-app.state"), "x").unwrap();
    std::fs::create_dir(root.join("state/other")).unwrap();
    assert!(
        woken.recv_timeout(QUIET).is_err(),
        "a parent kept watched after its folder appeared"
    );
    std::fs::write(dir.join("ui.json"), "{}").unwrap();
    assert!(
        woken.recv_timeout(GUARD).is_ok(),
        "the folder itself must still wake"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_repointed_folder_link_follows_its_new_target() {
    let root = scratch("relink");
    for target in ["a", "b"] {
        std::fs::create_dir(root.join(target)).unwrap();
    }
    let link = root.join("flea");
    std::os::unix::fs::symlink("a", &link).unwrap();
    let (send, woken) = mpsc::channel();
    let _wake = start("test", vec![link.clone()], UNREACHED_TICK, move || {
        send.send(()).is_ok()
    });
    // Replaced the way a dotfile manager does it: a new link renamed over the old one.
    std::os::unix::fs::symlink("b", root.join("flea.new")).unwrap();
    std::fs::rename(root.join("flea.new"), &link).unwrap();
    assert!(
        woken.recv_timeout(GUARD).is_ok(),
        "repointing the link must wake"
    );
    while woken.recv_timeout(QUIET).is_ok() {}
    std::fs::write(root.join("b/ui.json"), "{}").unwrap();
    assert!(
        woken.recv_timeout(GUARD).is_ok(),
        "a write in the new target must wake"
    );
    while woken.recv_timeout(QUIET).is_ok() {}
    std::fs::write(root.join("a/ui.json"), "{}").unwrap();
    assert!(
        woken.recv_timeout(QUIET).is_err(),
        "the old target must no longer wake"
    );
    let _ = std::fs::remove_dir_all(&root);
}

const TICK: Duration = Duration::from_millis(20);

fn ticks(woken: &mpsc::Receiver<()>) -> bool {
    (0..3).all(|_| woken.recv_timeout(GUARD).is_ok())
}

#[test]
fn a_folder_inotify_refuses_falls_back_to_the_tick_and_a_drop_still_ends_it() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("refused");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Root reads any folder, so the watch would succeed and there is nothing to refuse.
    if std::fs::read_dir(&dir).is_err() {
        let (send, woken) = mpsc::channel();
        let wake = start("test", vec![dir.clone()], TICK, move || {
            send.send(()).is_ok()
        });
        assert!(ticks(&woken), "a refused watch must keep the tick");
        drop(wake);
        assert!(
            ends(&woken),
            "dropping the handle must end a ticking thread"
        );
    }
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_inotify_the_tick_still_wakes_and_a_drop_still_ends_it() {
    let dir = scratch("noinotify");
    let (send, woken) = mpsc::channel();
    let wake = start_with(
        "test",
        vec![dir.clone()],
        TICK,
        move || send.send(()).is_ok(),
        false,
    );
    assert!(ticks(&woken), "without inotify the tick must still wake");
    drop(wake);
    assert!(
        ends(&woken),
        "dropping the handle must end a ticking thread"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
