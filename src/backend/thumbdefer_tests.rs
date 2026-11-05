// Thumbnail requests wider than the pool's queue: the rows that do not fit wait for a worker, none is answered empty.
use crate::backend::aliases::Aliases;
use crate::backend::archive::Formats;
use crate::backend::icons::Names;
use crate::backend::kind::Kinds;
use crate::backend::listing::Listing;
use crate::backend::mime::Db;
use crate::backend::state::{State, Tables};
use crate::backend::testdir::TestDir;
use crate::backend::thumbcache::Cache;
use crate::backend::run::forget_rows;
use crate::backend::thumbdefer::MAX_DEFERRED;
use crate::backend::thumbreq::{cancel_all, cancel_row, report_done, thumb_rows};
use crate::backend::thumbs::{Done, Outcome, Pool, MAX_QUEUE};
use std::path::PathBuf;
use crate::backend::thumbspec::Thumbnailers;
use std::cell::RefCell;
use std::os::unix::fs::MetadataExt;
use crate::backend::events::channel;
use std::sync::Arc;

// One jpeg declaration, so for_mime answers; the pool here has no worker, so the program it names never runs.
fn tables() -> Tables {
    let aliases = Arc::new(Aliases::load());
    let thumbs = Arc::new(Thumbnailers::from_entries(
        &[("t.thumbnailer".to_string(),
           "[Thumbnailer Entry]\nExec=/usr/bin/false %i %o %s\nMimeType=image/jpeg;\n".to_string())],
        &aliases,
    ));
    Tables {
        mime: Arc::new(Db::load()).into(),
        icons: Arc::new(Names::load()).into(),
        aliases: aliases.into(),
        thumbs: thumbs.into(),
        kinds: RefCell::new(Kinds::new()),
        formats: Arc::new(Formats::probe()).into(),
    }
}

/// A listing of `count` jpegs, each row carrying the figures the window holds for it, so no stat is taken.
fn listed(dir: &TestDir, st: &mut State, count: usize) {
    st.base = dir.path().to_path_buf();
    let mut listing = Listing::new();
    for row in 0..count {
        let name = format!("img-{row:03}.jpg");
        let path = dir.file(&name, "not a picture, and nothing here decodes it");
        let m = std::fs::symlink_metadata(&path).unwrap();
        st.window_meta.insert(row, (m.mode(), m.mtime(), false, 0));
        listing.push(&name, false);
    }
    st.listing = listing;
}

fn harness(dir: &TestDir) -> (State, Pool, Cache) {
    let (events, _) = channel();
    let st = State::new(crate::backend::dirsizeworker::Worker::new(events));
    (st, Pool::idle(), Cache::at(dir.join("cache")))
}

#[test]
fn a_request_wider_than_the_queue_answers_no_row_before_a_worker_runs() {
    // Sample input: MAX_QUEUE + 10 uncached rows named in one request, with no worker to run any of them.
    let d = TestDir::new("thumbdeferwide");
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    let count = MAX_QUEUE + 10;
    listed(&d, &mut st, count);
    let rows: Vec<usize> = (0..count).collect();
    let mut out = Vec::new();
    thumb_rows(&mut out, &rows, &mut st, &tb, &pool, &cache, false);
    assert!(out.is_empty(), "no row is answered before a worker runs, the rows that do not fit wait: {}",
        String::from_utf8_lossy(&out));
    assert_eq!(st.outstanding, count, "every row is still in flight");
}

// Every row a worker has run answers once, and the rows that waited are answered only after their job ran.
#[test]
fn every_row_of_a_wide_request_answers_once_its_job_has_run() {
    let d = TestDir::new("thumbdeferdrain");
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    let count = MAX_QUEUE + 10;
    listed(&d, &mut st, count);
    let rows: Vec<usize> = (0..count).collect();
    let mut out = Vec::new();
    thumb_rows(&mut out, &rows, &mut st, &tb, &pool, &cache, false);
    // Each batch is what the workers take off the pool, and every job in it reports at once.
    loop {
        let batch = pool.cancel_all();
        if batch.is_empty() {
            break;
        }
        for job in batch {
            report_done(&mut out, &mut st, &pool, Done { replies: None, mtime: job.mtime, path: job.path, trash_identity: job.trash_identity, result: Outcome::Failed, ms: 0.0, trace: None });
        }
    }
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), count, "one answer per row: {}", text);
    for row in 0..count {
        assert!(text.contains(&format!("\"row\":{},", row)), "row {} is answered", row);
    }
    assert_eq!(st.outstanding, 0, "nothing is left in flight");
    assert!(st.asked.is_empty(), "and no mapping is left behind");
    assert!(st.deferred.take_all().is_empty(), "and nothing waits for room");
}

// A listing change takes the rows still waiting for room with it, so none is submitted against the old row numbers.
#[test]
fn a_listing_change_takes_the_waiting_rows_with_it() {
    let d = TestDir::new("thumbdeferlist");
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    let count = MAX_QUEUE + 10;
    listed(&d, &mut st, count);
    let rows: Vec<usize> = (0..count).collect();
    let mut out = Vec::new();
    thumb_rows(&mut out, &rows, &mut st, &tb, &pool, &cache, false);
    forget_rows(&mut st, &pool);
    assert!(out.is_empty(), "a listing change answers nothing: {}", String::from_utf8_lossy(&out));
    assert_eq!(st.outstanding, 0, "every row, queued or waiting, is forgotten");
    assert!(st.asked.is_empty(), "and no mapping is left behind");
    assert!(pool.cancel_all().is_empty(), "the pool is empty");
    assert!(st.deferred.take_all().is_empty(), "and nothing waits for room");
}

// A cancel reaches a row that is still waiting for room, and the pool's own jobs stay as they were.
#[test]
fn a_cancelled_row_still_waiting_for_room_is_taken_back() {
    let d = TestDir::new("thumbdefercancel");
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    let count = MAX_QUEUE + 10;
    listed(&d, &mut st, count);
    let rows: Vec<usize> = (0..count).collect();
    let mut out = Vec::new();
    thumb_rows(&mut out, &rows, &mut st, &tb, &pool, &cache, false);
    cancel_row(&mut out, &mut st, &pool, MAX_QUEUE + 5);
    assert!(out.is_empty(), "a cancel answers nothing: {}", String::from_utf8_lossy(&out));
    assert_eq!(st.outstanding, count - 1, "the cancelled row is no longer in flight");
    assert!(st.asked.iter().all(|(_, r)| *r != MAX_QUEUE + 5), "and its mapping is gone");
    assert_eq!(pool.pending(), MAX_QUEUE, "the pool keeps its full queue");
    assert_eq!(st.deferred.take_all().len(), 9, "the other waiting rows stay waiting");
}

#[test]
fn a_pool_row_cancel_admits_the_oldest_deferred_job_without_a_report() {
    let d = TestDir::new("thumbdefercancelpump");
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    let count = MAX_QUEUE + 10;
    listed(&d, &mut st, count);
    let rows: Vec<usize> = (0..count).collect();
    let mut out = Vec::new();
    thumb_rows(&mut out, &rows, &mut st, &tb, &pool, &cache, false);
    assert_eq!(pool.pending(), MAX_QUEUE, "the pool is full before the queued row is cancelled");
    let oldest = d.join(&format!("img-{MAX_QUEUE:03}.jpg"));
    cancel_row(&mut out, &mut st, &pool, 0);
    assert_eq!(pool.pending(), MAX_QUEUE, "cancellation immediately gives the free slot to a waiting row");
    assert!(out.is_empty(), "cancelling a row writes no answer and no completion has been delivered");
    assert_eq!(st.outstanding, count - 1, "only the cancelled job leaves the outstanding count");
    assert!(st.asked.iter().all(|(_, row)| *row != 0), "the cancelled row loses its mapping");
    assert!(st.asked.iter().any(|(path, row)| *path == oldest && *row == MAX_QUEUE), "the admitted row keeps its mapping");
    let admitted = pool.cancel(&oldest, None);
    assert_eq!(admitted.len(), 1, "the oldest waiting job is now in the pool");
    assert_eq!(admitted[0].path, oldest);
    let waiting = st.deferred.take_all();
    assert_eq!(waiting.len(), 9, "the deferred queue is one job shorter");
    assert!(waiting.iter().all(|job| job.path != oldest), "the admitted job no longer waits");
    assert_eq!(waiting[0].path, d.join(&format!("img-{:03}.jpg", MAX_QUEUE + 1)), "the next oldest row stays at the FIFO head");
}

#[test]
fn cancel_all_forgets_deferred_rows_before_any_later_report() {
    let d = TestDir::new("thumbdefercancelall");
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    let count = MAX_QUEUE + 10;
    listed(&d, &mut st, count);
    let rows: Vec<usize> = (0..count).collect();
    let mut out = Vec::new();
    thumb_rows(&mut out, &rows, &mut st, &tb, &pool, &cache, false);
    assert_eq!(pool.pending(), MAX_QUEUE, "the request leaves ten rows waiting beyond the pool");
    let cancelled = d.join(&format!("img-{MAX_QUEUE:03}.jpg"));
    cancel_all(&mut st, &pool);
    assert_eq!(pool.pending(), 0, "cancel-all clears the pool");
    assert_eq!(st.asked.len(), 0, "no queued or deferred job stays mapped after cancel-all");
    assert_eq!(st.outstanding, 0, "queued and deferred jobs release their accounting");
    assert!(out.is_empty(), "cancel-all writes no answer");
    // Leave the deferred queue untouched before this report, so a leftover job would be submitted by the real pump.
    report_done(&mut out, &mut st, &pool, Done { replies: None,
        trash_identity: None,
        path: cancelled,
        mtime: 0,
        result: Outcome::Ready(d.join("cache/cancelled.png")),
        ms: 0.0,
        trace: None,
    });
    assert_eq!(pool.pending(), 0, "a later report admits no cancelled deferred job");
    assert!(out.is_empty(), "a cancelled row emits no thumbed line on a later report: {}", String::from_utf8_lossy(&out));
    assert!(st.asked.is_empty(), "no mapping is recreated by the report");
    assert_eq!(st.outstanding, 0, "the late report leaves no outstanding work");
    assert!(st.deferred.take_all().is_empty(), "no deferred job survived cancel-all");
}

// Past the deferred cap a row is answered empty at once, so the wait is bounded whatever a client asks.
#[test]
fn a_row_past_the_deferred_cap_is_answered_empty_at_once() {
    let d = TestDir::new("thumbdefercap");
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    let count = MAX_QUEUE + MAX_DEFERRED + 5;
    listed(&d, &mut st, count);
    let rows: Vec<usize> = (0..count).collect();
    let mut out = Vec::new();
    thumb_rows(&mut out, &rows, &mut st, &tb, &pool, &cache, false);
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 5, "exactly the five rows past the bound are answered: {}", text);
    assert!(text.contains(&format!("\"row\":{},", MAX_QUEUE + MAX_DEFERRED)), "the first refused row is answered");
    assert_eq!(st.outstanding, MAX_QUEUE + MAX_DEFERRED, "every row inside the bound is still in flight");
}

#[test]
fn a_full_deferred_queue_uses_freed_pool_room_before_refusing_a_new_row() {
    let d = TestDir::new("thumbdeferfreed");
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    let count = MAX_QUEUE + MAX_DEFERRED;
    listed(&d, &mut st, count + 1);
    let rows: Vec<usize> = (0..count).collect();
    let mut out = Vec::new();
    thumb_rows(&mut out, &rows, &mut st, &tb, &pool, &cache, false);
    assert!(out.is_empty(), "every row inside the bound waits");
    assert!(st.deferred.full(), "the deferred queue reached its cap");
    // Workers take jobs before their completions reach the loop, leaving pool room while the deferred queue is full.
    let running = pool.cancel_all();
    assert_eq!(running.len(), MAX_QUEUE);
    thumb_rows(&mut out, &[count], &mut st, &tb, &pool, &cache, false);
    assert!(out.is_empty(), "newly free room admits the new row instead of answering it empty: {}", String::from_utf8_lossy(&out));
    assert_eq!(st.outstanding, count + 1, "the new row joins the jobs still in flight");
    assert!(st.asked.iter().any(|(_, row)| *row == count), "the new row remains mapped");
    let waiting = pool.cancel_all();
    assert_eq!(waiting.len(), MAX_QUEUE, "waiting rows fill the newly freed pool room");
    for (row, job) in waiting.iter().enumerate() {
        assert_eq!(job.path, d.join(&format!("img-{:03}.jpg", MAX_QUEUE + row)), "the oldest waiting rows enter the pool first");
    }
    let deferred = st.deferred.take_all();
    assert_eq!(deferred.len(), MAX_DEFERRED - MAX_QUEUE + 1);
    assert_eq!(deferred.last().unwrap().path, d.join(&format!("img-{count:03}.jpg")), "the new row waits at the FIFO tail");
}

// A listing change renumbers rows, so a job still running for a path answers that path's row by its own mapping only.
#[test]
fn a_path_asked_again_after_a_listing_change_answers_its_new_row() {
    let d = TestDir::new("thumbdeferrenumber");
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    listed(&d, &mut st, 2);
    let mut out = Vec::new();
    thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, false);
    // A worker takes the first job, and it keeps running after the listing changes.
    let running = pool.cancel_all();
    assert_eq!(running.len(), 1, "one job is in the pool");
    forget_rows(&mut st, &pool);
    // A re-sort puts img-000.jpg at row 1, and the client asks for it there.
    let mut resorted = Listing::new();
    resorted.push("img-001.jpg", false);
    resorted.push("img-000.jpg", false);
    st.listing = resorted;
    thumb_rows(&mut out, &[1], &mut st, &tb, &pool, &cache, false);
    assert_eq!(pool.pending(), 1, "the path is queued again, not blocked by the job still running");
    report_done(&mut out, &mut st, &pool, Done { replies: None, mtime: running[0].mtime, path: running[0].path.clone(), trash_identity: None, result: Outcome::Ready(PathBuf::from("/cache/a.png")), ms: 0.0, trace: None });
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("\"row\":1,\"file\":\"/cache/a.png\""), "the new row takes the answer: {}", text);
    assert!(!text.contains("\"row\":0,"), "the old row number is never answered: {}", text);
}
