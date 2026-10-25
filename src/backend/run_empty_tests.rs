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
        pool.submit(super::super::thumbs::Job { path: path.clone(), mtime: 0, mime: "application/x-flea-no-thumbnailer".into(), trace: None, trash_identity: None });
        let reply = replies.recv_timeout(Duration::from_secs(5)).expect("each lazy job reaches the loop");
        assert!(matches!(reply, Event::Thumb(Done { path: got, result: super::super::thumbs::Outcome::Failed, .. }) if got == path));
    }
}
