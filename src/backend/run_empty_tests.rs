use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn run_search_empty_windows_leave_tables_unloaded_and_preserve_rich_rows() {
    let d = TestDir::new("empty-window-tables");
    d.file("hello.txt", "hello");
    d.dir("folder");
    let tb = Tables::load();
    let (events, _) = crate::backend::events::channel();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events));
    st.base = d.path().to_path_buf();
    st.listing.push("hello.txt", false);
    st.listing.push("folder", true);
    let mut out = Vec::new();
    write_window_gen(&mut out, &mut st, 999, 0, &tb, Some(7));
    assert_eq!(String::from_utf8(out).unwrap(), "{\"t\":\"rows\",\"start\":2,\"rows\":[],\"kinds\":[],\"ms\":0.000,\"listing\":0,\"gen\":7}\n");
    assert!(tb.mime.get().is_none() && tb.icons.get().is_none());
    assert!(tb.aliases.get().is_none() && tb.thumbs.get().is_none() && tb.formats.get().is_none());
    assert_eq!(rows_needed(&Request::Window { start: 999, count: 0 }), 0);
    let (metas, _) = window_metas(&mut st, 0, 2).unwrap();
    let mime = super::super::mime::Db::load();
    let icons = super::super::icons::Names::load();
    let aliases = super::super::aliases::Aliases::load();
    let thumbs = super::super::thumbspec::Thumbnailers::load(&aliases);
    let mut kinds = super::super::kind::Kinds::new();
    let expected = super::super::rows::rows_line(&st.base, &st.listing, &metas, 0, 0.0,
        &mime, &icons, &aliases, &thumbs, &mut kinds);
    let actual = tb.rows_line(&st.base, &st.listing, &metas, 0, 0.0);
    assert_eq!(actual, expected, "lazy tables preserve every complete row field");
    assert!(tb.mime.get().is_some() && tb.icons.get().is_some());
    assert!(tb.aliases.get().is_some() && tb.thumbs.get().is_some() && tb.formats.get().is_none());
}

#[test]
fn run_search_empty_remote_window_never_enters_the_mount_worker() {
    let (events, _) = crate::backend::events::channel();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events));
    st.base = PathBuf::from("/run/user/flea-empty-test/gvfs/smb-share:server=test,share=wire");
    let before = super::super::iomount::test_calls();
    let (metas, ms) = window_metas(&mut st, 0, 0).unwrap();
    assert!(metas.is_empty());
    assert_eq!(ms, 0.0);
    assert_eq!(super::super::iomount::test_calls(), before);
}

/// Verify that a search can request its first thumbnail through the lazy pool.
#[test]
fn run_search_lazy_thumbnail_pool_reports_its_first_job() {
    const CHILD: &str = "FLEA_TEST_DISABLED_THUMB_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "backend::run::empty_tests::run_search_lazy_thumbnail_pool_reports_its_first_job", "--nocapture"])
            .env(CHILD, "1").env("PATH", "").output().unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "{stderr}");
        assert_eq!(stderr.matches("flea: thumbnails are disabled, bwrap or prlimit is not on PATH").count(), 1, "{stderr}");
        return;
    }
    let d = TestDir::new("lazy-thumbnail-pool");
    let tb = Tables::load();
    let (events, replies) = crate::backend::events::channel();
    let cache_root = d.join("thumb-cache");
    d.assert_contains(&cache_root);
    let pool = Pool::lazy(1, &tb, cache_root, events);
    assert_eq!(pool.pending(), 0);
    assert!(pool.cancel_all().is_empty());
    assert!(tb.aliases.get().is_none() && tb.thumbs.get().is_none());
    for name in ["first-not-decoded", "second-not-decoded"] {
        let path = d.join(name);
        pool.submit(super::super::thumbs::Job { replies: None, path: path.clone(), mtime: 0, source_size: None, mime: "application/x-flea-no-thumbnailer".into(), trace: None, trash_identity: None });
        let reply = replies.recv_timeout(Duration::from_secs(5)).expect("each lazy job reaches the loop");
        assert!(matches!(reply, Event::Thumb(Done { path: got, result: super::super::thumbs::Outcome::Failed, .. }) if got == path));
    }
}

#[test]
fn a_remote_window_whose_stat_fails_answers_its_rows_then_says_why() {
    let (events, _) = crate::backend::events::channel();
    let tb = Tables::load();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events));
    st.base = PathBuf::from("/run/user/flea-empty-test/gvfs/smb-share:server=test,share=lost");
    st.listing.push("a.txt", false);
    st.listing.push("b.txt", false);
    let mount = super::super::iomount::mount_key(&st.base, "");
    let _stuck = crate::backend::iomount::test_hold_stuck(mount);
    let mut out = Vec::new();
    write_window_gen(&mut out, &mut st, 0, 2, &tb, Some(7));
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "one rows line, then one error line: {text}");
    assert!(lines[0].starts_with(r#"{"t":"rows","start":0"#), "{text}");
    assert!(lines[0].contains(r#""n":"a.txt""#) && lines[0].contains(r#""n":"b.txt""#), "{text}");
    assert!(lines[1].starts_with(r#"{"t":"error""#) && lines[1].contains("is not responding."), "{text}");
    assert!(st.window_meta.is_empty(), "zeroes from a failed stat are never cached: {:?}", st.window_meta);
}

#[test]
fn a_remote_window_asked_again_after_its_mount_answers_reads_real_metadata() {
    let d = TestDir::new("window-recovers");
    let share = d.dir("run/1000/gvfs/smb-share:server=test,share=recover");
    std::fs::write(share.join("a.txt"), "a").unwrap();
    std::fs::write(share.join("b.txt"), "bb").unwrap();
    crate::backend::gvfsroot::set_test_run_user(Some(&d.join("run")));
    let (events, _) = crate::backend::events::channel();
    let tb = Tables::load();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events));
    st.base = share.clone();
    st.listing.push("a.txt", false);
    st.listing.push("b.txt", false);
    let mount = super::super::iomount::mount_key(&st.base, "");
    let stuck = crate::backend::iomount::test_hold_stuck(mount);
    let mut out = Vec::new();
    write_window_gen(&mut out, &mut st, 0, 2, &tb, Some(7));
    assert_eq!(String::from_utf8(out).unwrap().lines().count(), 2, "the stuck window says why");
    assert!(st.window_meta.is_empty(), "the failed window caches nothing: {:?}", st.window_meta);
    drop(stuck);
    let mut out = Vec::new();
    write_window_gen(&mut out, &mut st, 0, 2, &tb, Some(7));
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 1, "the mount answers again, so no error line: {text}");
    let real = st.window_meta.get(&1).expect("the second window caches the row it stat'd");
    assert_eq!(real.0 & 0o170000, 0o100000, "a regular file's mode: {:?}", st.window_meta);
    assert_ne!(real.1, 0, "a real mtime: {:?}", st.window_meta);
    crate::backend::gvfsroot::set_test_run_user(None);
}
