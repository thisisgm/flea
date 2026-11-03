use super::*;
use crate::json::escape;
use crate::backend::run::forget_rows;
use crate::backend::testdir::TestDir;
use crate::backend::thumbspec::Thumbnailers;
use crate::backend::trashdelete::Reviewed;
use crate::backend::events::channel;

fn trash_request(out: &mut Vec<u8>, st: &mut State, tb: &Tables, pool: &Pool, cache: &Cache, path: &Path, id: usize) {
    let meta = path.symlink_metadata().unwrap();
    let provider = format!("l{}:{}", meta.dev(), meta.ino());
    let reviewed = Reviewed::inspect(path.to_path_buf(), &provider).unwrap();
    let line = format!(r#"{{"c":"trashthumb","id":{},"original":"/original/photo.jpg","path":"{}","provider":"{}","identity":"{}"}}"#,
        id, escape(&path.to_string_lossy()), provider, escape(&reviewed.selection_identity()));
    crate::backend::trashthumb::request(out, st, tb, pool, cache, &line);
}

fn cancel_after_an_older_completion_orphans_the_new_job(deferred: bool) {
    let d = TestDir::new("thumbcancel-orphan");
    d.dir("files");
    d.dir("info");
    let mut tb = Tables::load();
    tb.thumbs = std::sync::Arc::new(Thumbnailers::from_entries(&[("t.thumbnailer".into(),
        "[Thumbnailer Entry]\nExec=/usr/bin/false %i %o %s\nMimeType=image/jpeg;\n".into())], &tb.aliases)).into();
    let (events, _) = channel();
    let mut st = State::new(crate::backend::dirsizeworker::Worker::new(events));
    st.base = d.join("files");
    let count = if deferred { crate::backend::thumbs::MAX_QUEUE + 1 } else { 2 };
    for row in 0..count {
        let name = format!("img-{row:03}.jpg");
        d.file(&format!("files/{name}"), "test picture bytes");
        st.listing.push(&name, false);
        if row < 2 { d.file(&format!("info/{name}.trashinfo"), "[Trash Info]\nPath=/original/photo.jpg\n"); }
    }
    let path = st.base.join("img-000.jpg");
    let control = st.base.join("img-001.jpg");
    let pool = Pool::idle();
    let cache = Cache::at(d.join("cache"));
    let mut out = Vec::new();
    thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, false);
    let old = pool.cancel(&path, None).pop().unwrap();
    trash_request(&mut out, &mut st, &tb, &pool, &cache, &path, 10);
    assert_eq!(st.trash_asked.len(), 1);
    forget_rows(&mut st, &pool);
    assert_eq!(pool.pending(), 1, "relisting preserves the first Trash job");
    thumb_rows(&mut out, &[1], &mut st, &tb, &pool, &cache, false);
    let running = pool.cancel(&control, None).pop().unwrap();
    if deferred {
        let rows: Vec<_> = (2..count).collect();
        thumb_rows(&mut out, &rows, &mut st, &tb, &pool, &cache, false);
        assert_eq!(pool.pending(), crate::backend::thumbs::MAX_QUEUE);
    }
    thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, false);
    assert_eq!(st.thumb_mtimes.get(&path), Some(&old.mtime));
    trash_request(&mut out, &mut st, &tb, &pool, &cache, &control, 11);
    assert_eq!(st.trash_asked.len(), 2, "Trash and ordinary jobs use the same two paths");
    report_done(&mut out, &mut st, &pool, Done { replies: None, path: old.path, mtime: old.mtime, trash_identity: old.trash_identity,
        result: Outcome::Failed, ms: 0.0, trace: None });
    assert!(st.asked.iter().all(|(p, _)| p != &path), "the predecessor consumed the new same-stamp mapping");
    assert!(!st.thumb_mtimes.contains_key(&path));
    assert_eq!(out.split(|b| *b == b'\n').filter(|line| !line.is_empty()).count(), 1);
    let before = st.outstanding;
    cancel_all(&mut st, &pool);
    let queued = pool.cancel_all();
    let waiting = st.deferred.take_all();
    assert!(queued.iter().chain(&waiting).all(|j| j.trash_identity.is_some()),
        "cancel-all left an ordinary orphan alive, deferred={deferred}");
    assert_eq!(queued.len(), if deferred { 1 } else { 2 }, "queued Trash jobs survive");
    assert_eq!(waiting.len(), usize::from(deferred), "deferred Trash jobs survive");
    assert_eq!(queued[0].path, path, "the oldest Trash job retains its position");
    assert_eq!(queued.iter().chain(&waiting).last().unwrap().path, control);
    assert_eq!(st.trash_asked.len(), 2, "both Trash subscriptions survive");
    assert_eq!(st.outstanding, 3, "only the two Trash jobs and running ordinary job remain");
    assert_eq!(before - st.outstanding, if deferred { crate::backend::thumbs::MAX_QUEUE } else { 1 });
    assert_eq!(st.asked, vec![(control.clone(), 1)], "a running ordinary job retains its row");
    assert_eq!(st.thumb_mtimes.len(), 1);
    assert_eq!(st.thumb_mtimes.get(&control), Some(&running.mtime));
    for job in queued { assert!(pool.submit(job).is_empty()); }
    for job in waiting { st.deferred.enqueue(job); }
    out.clear();
    report_done(&mut out, &mut st, &pool, Done { replies: None, path: running.path, mtime: running.mtime, trash_identity: running.trash_identity,
        result: Outcome::Failed, ms: 0.0, trace: None });
    assert_eq!(String::from_utf8(out).unwrap(), format!("{}\n", thumbed_line(1, "", 0.0)));
    assert_eq!(st.outstanding, 2);
    assert!(st.asked.is_empty());
    assert!(st.thumb_mtimes.is_empty());
    assert_eq!(pool.pending(), 2, "a later report pumps only preserved Trash work");
    assert!(st.deferred.take_all().is_empty());
    assert_eq!(st.trash_asked.len(), 2);
}

#[test]
fn cancel_all_drains_orphaned_ordinary_pool_jobs_without_cancelling_trash_or_running_jobs() {
    cancel_after_an_older_completion_orphans_the_new_job(false);
}

#[test]
fn cancel_all_drains_orphaned_ordinary_deferred_jobs_without_cancelling_trash_or_running_jobs() {
    cancel_after_an_older_completion_orphans_the_new_job(true);
}
