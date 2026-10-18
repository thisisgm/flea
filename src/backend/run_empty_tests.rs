use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn run_search_empty_windows_leave_tables_unloaded_and_preserve_rich_rows() {
    let d = TestDir::new("empty-window-tables");
    d.file("hello.txt", "hello");
    d.dir("folder");
    let tb = Tables::load();
    let (events, _) = channel();
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
    let (metas, _) = window_metas(&st, 0, 2).unwrap();
    let actual = tb.rows_line(&st.base, &st.listing, &metas, 0, 0.0);
    let expected = super::super::rows::rows_line(&st.base, &st.listing, &metas, 0, 0.0,
        &tb.mime, &tb.icons, &tb.aliases, &tb.thumbs, &mut tb.kinds.borrow_mut());
    assert_eq!(actual, expected, "lazy tables preserve every complete row field");
    assert!(tb.mime.get().is_some() && tb.icons.get().is_some());
    assert!(tb.aliases.get().is_some() && tb.thumbs.get().is_some() && tb.formats.get().is_none());
}

#[test]
fn run_search_empty_remote_window_never_enters_the_mount_worker() {
    let (events, _) = channel();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events));
    st.base = PathBuf::from("/run/user/flea-empty-test/gvfs/smb-share:server=test,share=wire");
    let before = super::super::iomount::test_calls();
    let (metas, ms) = window_metas(&st, 0, 0).unwrap();
    assert!(metas.is_empty());
    assert_eq!(ms, 0.0);
    assert_eq!(super::super::iomount::test_calls(), before);
}

#[test]
fn run_search_lazy_thumbnail_pool_reports_its_first_job() {
    let d = TestDir::new("lazy-thumbnail-pool");
    let tb = Tables::load();
    let (events, replies) = channel();
    let pool = Pool::lazy(1, &tb, events);
    assert_eq!(pool.pending(), 0);
    assert!(pool.cancel_all().is_empty());
    assert!(tb.aliases.get().is_none() && tb.thumbs.get().is_none());
    let path = d.join("not-decoded");
    pool.submit(super::super::thumbs::Job { path: path.clone(), mtime: 0, mime: "application/x-flea-no-thumbnailer".into(), trace: None });
    let reply = replies.recv_timeout(Duration::from_secs(5)).expect("the first lazy job reaches the loop");
    assert!(matches!(reply, Event::Thumb(Done { path: got, result: super::super::thumbs::Outcome::Failed, .. }) if got == path));
}

