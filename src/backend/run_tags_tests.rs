// The loop's tag probe on fsinfo, driven through the production dispatcher; beside run.rs for its file budget.
use super::*;
use crate::backend::tags::PROBES;
use crate::backend::testdir::TestDir;

// Asks fsinfo `asks` times at base, answering every line and this thread's probe count since the first ask.
fn ask_fsinfo(sandbox: &TestDir, base: &Path, asks: usize) -> (String, usize) {
    let tb = Tables::load();
    let (events, _events_rx) = crate::backend::events::channel();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    st.base = base.to_path_buf();
    let (results, _done) = channel();
    // Inside the test's own sandbox, so parallel runs share nothing and drop removes it.
    let cache_root = sandbox.join("fsinfo-tags-cache");
    sandbox.assert_contains(&cache_root);
    let pool = Pool::new(1, results, cache_root.clone(), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
    let cache = Cache::at(cache_root);
    let (tx, _rx) = channel();
    let mut ops = Ops::new(tx);
    let (flash_tx, _flash_rx) = channel();
    let mut flash = flashslot::Ctx::new(flash_tx);
    let (mut watch, mut peeks) = (Watch::start(events.clone()), PeekWatch::start(events.clone()));
    let mut fsinfo = FsInfo::new(events.clone());
    let poller = super::super::watchpoll::Poller::new(events.clone());
    let mut out = Vec::new();
    PROBES.with(|probes| probes.set(0));
    for _ in 0..asks {
        assert!(handle_line(r#"{"c":"fsinfo"}"#, &mut out, &mut st, &tb, &pool, &cache, &mut ops, &mut flash,
            &mut watch, &mut peeks, &mut fsinfo, &poller, &events, &mut Gate::new(events.clone())) == Control::Continue);
    }
    (String::from_utf8(out).expect("the fsinfo wire is UTF-8"), PROBES.with(|probes| probes.get()))
}

#[test]
fn a_repeated_fsinfo_probes_a_local_directory_once() {
    let d = TestDir::new("fsinfotags");
    let (lines, probes) = ask_fsinfo(&d, d.path(), 3);
    assert_eq!(lines.matches(r#""tagable":true"#).count(), 3, "{}", lines);
    assert_eq!(probes, 1, "one xattr probe per directory, however often fsinfo is asked");
}

#[test]
fn a_slow_mount_is_never_probed_on_the_loop() {
    let d = TestDir::new("fsinfotagsslow");
    let (lines, probes) = ask_fsinfo(&d, Path::new("/run/user/1000/gvfs/smb-share:server=flea-fake,share=tags"), 2);
    assert_eq!(lines.matches(r#""tagable":true"#).count(), 2, "{}", lines);
    assert_eq!(probes, 0, "a share's probe runs on the fsinfo worker beside its statfs");
}
