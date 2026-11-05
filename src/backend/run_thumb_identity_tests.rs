//! Optional identity through the real parser, dispatch, queued/running jobs and reply emitter.
use super::*;
use crate::backend::{listing::Listing, thumbs::Outcome, thumbspec::Thumbnailers};

fn fixture(tag: &str) -> Loop {
    let mut l = Loop::new(tag);
    l.pool = Pool::idle();
    l.tb.thumbs = Arc::new(Thumbnailers::from_entries(
        &[(
            "t.thumbnailer".into(),
            "[Thumbnailer Entry]\nExec=/usr/bin/false %i %o %s\nMimeType=image/jpeg;\n".into(),
        )],
        &l.tb.aliases,
    ))
    .into();
    l.st.generation = 1;
    l.st.base = l.dir.path().to_path_buf();
    for name in ["old.jpg", "new.jpg"] {
        l.dir.file(name, "not decoded by the idle worker");
    }
    l.st.listing.push("old.jpg", false);
    l
}

fn ask(l: &mut Loop, token: &str, name: &str) {
    let path = l.dir.join(name);
    let listing = l.st.generation;
    assert!(l.request(&format!(r#"{{"c":"thumb","rows":[0],"thumbToken":"{token}","thumbPath":"{}","thumbListing":{listing}}}"#,
        crate::json::escape(&path.to_string_lossy()))) == Control::Continue);
}

#[test]
fn policy_identity_running_old_different_path_is_dropped_and_new_completion_keeps_tag() {
    let mut l = fixture("thumb-owner-running");
    ask(&mut l, "old-owner", "old.jpg");
    let old = l.pool.cancel_all().pop().unwrap();
    assert_eq!(
        l.st.outstanding, 1,
        "worker holds the old job outside the queue"
    );
    forget_rows(&mut l.st, &l.pool);
    l.st.listing = Listing::new();
    l.st.listing.push("new.jpg", false);
    ask(&mut l, "new-owner", "new.jpg");
    let new = l.pool.cancel_all().pop().unwrap();
    report_done(
        &mut l.out,
        &mut l.st,
        &l.pool,
        Done {
            replies: old.replies,
            path: old.path,
            mtime: old.mtime,
            trash_identity: old.trash_identity,
            result: Outcome::Ready(l.dir.join("old-cache.png")),
            ms: 0.0,
            trace: old.trace,
        },
    );
    assert!(
        l.out.is_empty(),
        "real listing retirement drops the old different-path completion"
    );
    report_done(
        &mut l.out,
        &mut l.st,
        &l.pool,
        Done {
            replies: new.replies,
            path: new.path,
            mtime: new.mtime,
            trash_identity: new.trash_identity,
            result: Outcome::Ready(l.dir.join("new-cache.png")),
            ms: 0.0,
            trace: new.trace,
        },
    );
    let line = l.said();
    let tagged = line
        .lines()
        .find(|line| line.contains("thumbToken"))
        .expect("tagged completion");
    let doc = crate::jsondoc::parse(tagged).unwrap();
    assert_eq!(
        doc.get("thumbToken").and_then(crate::jsondoc::Json::as_str),
        Some("new-owner"),
        "accepted completion lost immutable request ownership: {line}"
    );
}

fn finish(l: &mut Loop, job: crate::backend::thumbs::Job, file: Option<PathBuf>) {
    report_done(
        &mut l.out,
        &mut l.st,
        &l.pool,
        Done {
            replies: job.replies,
            path: job.path,
            mtime: job.mtime,
            trash_identity: job.trash_identity,
            result: file.map_or(Outcome::Failed, Outcome::Ready),
            ms: 0.0,
            trace: job.trace,
        },
    );
}

fn tagged(l: &mut Loop) -> Vec<crate::jsondoc::Json> {
    l.said()
        .lines()
        .filter_map(|line| crate::jsondoc::parse(line).ok())
        .filter(|doc| doc.get("thumbToken").is_some())
        .collect()
}

#[test]
fn policy_identity_duplicate_subscriptions_do_not_add_jobs_and_cancel_retires_old_token() {
    let mut l = fixture("thumb-owner-shared");
    ask(&mut l, "old", "old.jpg");
    ask(&mut l, "duplicate", "old.jpg");
    assert_eq!(l.st.outstanding, 1);
    let running = l.pool.cancel_all().pop().unwrap();
    assert!(l.request(r#"{"c":"thumbcancel","rows":[0]}"#) == Control::Continue);
    assert_eq!(
        l.st.outstanding, 1,
        "running job still owns original scheduling/accounting"
    );
    ask(&mut l, "new", "old.jpg");
    assert_eq!(l.pool.pending(), 0, "the actual running job is shared");
    let path = l.dir.join("raw.png");
    finish(&mut l, running, Some(path.clone()));
    let docs = tagged(&mut l);
    assert_eq!(docs.len(), 1);
    assert_eq!(
        docs[0]
            .get("thumbToken")
            .and_then(crate::jsondoc::Json::as_str),
        Some("new")
    );
    assert_eq!(
        docs[0]
            .get("thumbPath")
            .and_then(crate::jsondoc::Json::as_str),
        l.dir.join("old.jpg").to_str()
    );
    assert_eq!(
        docs[0].get("file").and_then(crate::jsondoc::Json::as_str),
        path.to_str()
    );
}

#[test]
fn policy_identity_same_path_new_listing_cannot_authenticate_from_predecessor_job() {
    let mut l = fixture("thumb-owner-same-path");
    ask(&mut l, "old", "old.jpg");
    let old = l.pool.cancel_all().pop().unwrap();
    forget_rows(&mut l.st, &l.pool);
    ask(&mut l, "new", "old.jpg");
    let new = l.pool.cancel_all().pop().unwrap();
    let old_file = l.dir.join("old.png");
    finish(&mut l, old, Some(old_file));
    assert!(
        tagged(&mut l).is_empty(),
        "old context cannot publish under a mutable replacement mapping"
    );
    let new_file = l.dir.join("new.png");
    finish(&mut l, new, Some(new_file.clone()));
    let docs = tagged(&mut l);
    assert_eq!(docs.len(), 1);
    assert_eq!(
        docs[0]
            .get("thumbToken")
            .and_then(crate::jsondoc::Json::as_str),
        Some("new")
    );
    assert_eq!(
        docs[0].get("thumbListing"),
        Some(&crate::jsondoc::Json::Num("2".into()))
    );
    assert_eq!(
        docs[0].get("file").and_then(crate::jsondoc::Json::as_str),
        new_file.to_str()
    );
}

/// Check the replies for cache hits, empty results and failures with or without an identity.
#[test]
fn policy_identity_cache_hit_empty_failure_and_legacy_reply_values_are_truthful() {
    use std::os::unix::fs::MetadataExt;
    let mut l = fixture("thumb-owner-cache");
    let path = l.dir.join("old.jpg");
    let mtime = path.metadata().unwrap().mtime();
    l.st.window_meta.insert(0, (0o100644, mtime, false, 0));
    let uri = crate::backend::thumbcache::uri_for(&path);
    let cached = l.cache.large_path(&uri);
    std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
    crate::backend::thumbwrite::write_marker(&cached, &uri, mtime, None).unwrap();
    ask(&mut l, "cache-hit", "old.jpg");
    assert_eq!(l.st.outstanding, 0);
    let docs = tagged(&mut l);
    assert_eq!(docs.len(), 1);
    assert_eq!(
        docs[0].get("file").and_then(crate::jsondoc::Json::as_str),
        cached.to_str()
    );
    // An unsupported file answers raw empty without any queued decoder.
    l.dir.file("data.unknown", "data");
    l.st.listing = Listing::new();
    l.st.listing.push("data.unknown", false);
    ask(&mut l, "empty", "data.unknown");
    assert_eq!(
        tagged(&mut l)[0]
            .get("file")
            .and_then(crate::jsondoc::Json::as_str),
        Some("")
    );
    assert_eq!(l.pool.pending(), 0);
    // Untagged clients retain the exact original object, including its field order and timing format.
    assert!(l.request(r#"{"c":"thumb","rows":[0]}"#) == Control::Continue);
    let legacy = l.said();
    let doc = crate::jsondoc::parse(legacy.trim()).unwrap();
    let ms = doc
        .get("ms")
        .and_then(crate::jsondoc::Json::as_f64)
        .unwrap();
    assert_eq!(legacy, format!("{}\n", thumbed_line(0, "", ms)));
    l.st.listing = Listing::new();
    l.st.listing.push("new.jpg", false);
    l.st.window_meta.clear();
    ask(&mut l, "worker-failure", "new.jpg");
    let job = l.pool.cancel_all().pop().unwrap();
    finish(&mut l, job, None);
    let docs = tagged(&mut l);
    assert_eq!(
        docs[0]
            .get("thumbToken")
            .and_then(crate::jsondoc::Json::as_str),
        Some("worker-failure")
    );
    assert_eq!(
        docs[0].get("file").and_then(crate::jsondoc::Json::as_str),
        Some("")
    );
}

#[test]
fn policy_identity_deferred_request_keeps_its_capture_and_cancelled_queue_stays_silent() {
    let mut l = fixture("thumb-owner-deferred");
    for row in 1..=crate::backend::thumbs::MAX_QUEUE {
        let name = format!("extra-{row}.jpg");
        l.dir.file(&name, "bytes");
        l.st.listing.push(&name, false);
    }
    let rows: Vec<_> = (1..=crate::backend::thumbs::MAX_QUEUE).collect();
    thumb_rows(
        &mut l.out, &rows, &mut l.st, &l.tb, &l.pool, &l.cache, false,
    );
    ask(&mut l, "deferred", "old.jpg");
    assert_eq!(l.pool.pending(), crate::backend::thumbs::MAX_QUEUE);
    let running = l.pool.cancel_all();
    crate::backend::thumbdefer::pump(&mut l.out, &mut l.st, &l.pool);
    let deferred = l.pool.cancel_all().pop().unwrap();
    assert_eq!(deferred.path, l.dir.join("old.jpg"));
    finish(&mut l, deferred, None);
    assert_eq!(
        tagged(&mut l)[0]
            .get("thumbToken")
            .and_then(crate::jsondoc::Json::as_str),
        Some("deferred")
    );
    // New queued work is removed by the original cancellation path, and cannot publish later.
    ask(&mut l, "cancelled", "old.jpg");
    assert_eq!(l.pool.pending(), 1);
    assert!(l.request(r#"{"c":"thumbcancel","rows":[0]}"#) == Control::Continue);
    assert_eq!(l.pool.pending(), 0);
    assert!(tagged(&mut l).is_empty());
    drop(running);
}

#[test]
fn policy_identity_partial_foreign_and_stale_requests_do_not_fall_back_to_legacy() {
    let mut l = fixture("thumb-owner-invalid");
    for line in [
        r#"{"c":"thumb","rows":[0],"thumbToken":"partial"}"#,
        r#"{"c":"thumb","rows":[0],"thumbToken":"foreign","thumbPath":"/foreign.jpg","thumbListing":1}"#,
        r#"{"c":"thumb","rows":[0],"thumbToken":"stale","thumbPath":"/foreign.jpg","thumbListing":999}"#,
    ] {
        assert!(l.request(line) == Control::Continue);
        assert_eq!(l.pool.pending(), 0);
        assert!(!l.said().contains("thumbed"));
    }
}

#[test]
fn policy_identity_running_job_accepts_new_subscription_after_cancel_all_without_rescheduling() {
    let mut l = fixture("thumb-owner-cancel-all-running");
    ask(&mut l, "old", "old.jpg");
    let running = l.pool.cancel_all().pop().unwrap();
    assert!(l.request(r#"{"c":"thumbcancel","rows":[]}"#) == Control::Continue);
    ask(&mut l, "new", "old.jpg");
    assert_eq!(l.pool.pending(), 0);
    assert_eq!(l.st.outstanding, 1);
    finish(&mut l, running, None);
    let docs = tagged(&mut l);
    assert_eq!(docs.len(), 1);
    assert_eq!(
        docs[0]
            .get("thumbToken")
            .and_then(crate::jsondoc::Json::as_str),
        Some("new")
    );
}

#[test]
fn policy_identity_disconnected_drain_keeps_queued_request_identity_with_raw_empty() {
    let mut l = fixture("thumb-owner-drain");
    ask(&mut l, "drain", "old.jpg");
    assert_eq!(l.pool.pending(), 1);
    let (tx, rx) = channel();
    drop(tx);
    drain(
        &mut l.out,
        &mut l.st,
        &mut l.ops,
        &mut l.flash,
        &rx,
        &l.pool,
        &l.cache,
    );
    assert_eq!(l.pool.pending(), 0);
    let docs = tagged(&mut l);
    assert_eq!(
        docs.len(),
        1,
        "drain must retain the queued request context through queue removal"
    );
    assert_eq!(
        docs[0]
            .get("thumbToken")
            .and_then(crate::jsondoc::Json::as_str),
        Some("drain")
    );
    assert_eq!(
        docs[0].get("file").and_then(crate::jsondoc::Json::as_str),
        Some("")
    );
}

#[path = "run_thumb_identity_stale_tests.rs"]
mod stale_tests;

#[path = "run_thumb_video_tests.rs"]
mod video_tests;
