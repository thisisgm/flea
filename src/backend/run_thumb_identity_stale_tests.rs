//! Metadata refresh keeps the listing but cannot make predecessor work a fresh owned result.
use super::*;
use std::{
    fs::{File, FileTimes},
    time::{Duration, UNIX_EPOCH},
};

#[test]
fn policy_identity_changed_metadata_cannot_retag_predecessor_cache_as_current() {
    let mut l = fixture("thumb-owner-metadata-refresh");
    let path = l.dir.join("old.jpg");
    let set_mtime = |seconds| {
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
            .unwrap()
    };
    set_mtime(10);
    assert!(l.request(r#"{"c":"window","start":0,"count":1}"#) == Control::Continue);
    let rows = l.said();
    let flea::protocol::Reply::Rows { rows, .. } =
        flea::protocol::parse_reply(rows.trim()).unwrap()
    else {
        panic!("actual window rows")
    };
    assert_eq!(rows[0].mtime, 10);
    ask(&mut l, "old", "old.jpg");
    let running = l.pool.cancel_all().pop().unwrap();
    assert_eq!(running.mtime, 10);
    let uri = crate::backend::thumbcache::uri_for(&path);
    let old_cache = l.cache.large_path(&uri);
    std::fs::create_dir_all(old_cache.parent().unwrap()).unwrap();
    crate::backend::thumbwrite::write_marker(&old_cache, &uri, 10, None).unwrap();
    assert!(l.request(r#"{"c":"thumbcancel","rows":[0]}"#) == Control::Continue);
    set_mtime(20);
    let listing = l.st.generation;
    assert!(l.request(r#"{"c":"window","start":0,"count":1}"#) == Control::Continue);
    let rows = l.said();
    let flea::protocol::Reply::Rows { rows, .. } =
        flea::protocol::parse_reply(rows.trim()).unwrap()
    else {
        panic!("refreshed window rows")
    };
    assert_eq!(rows[0].mtime, 20);
    assert!(
        l.st.window_meta.is_empty(),
        "this real thumb request takes the fresh-stat path"
    );
    assert_eq!(
        l.st.generation, listing,
        "real metadata refresh does not retire the listing"
    );
    assert!(l.request(r#"{"c":"thumb","rows":[0]}"#) == Control::Continue);
    assert!(
        l.said().is_empty(),
        "legacy duplicate keeps its original behavior"
    );
    ask(&mut l, "new", "old.jpg");
    ask(&mut l, "new", "old.jpg");
    assert_eq!(l.pool.pending(), 0);
    assert_eq!(
        l.st.outstanding, 1,
        "identity validation cannot schedule another job"
    );
    finish(&mut l, running, Some(old_cache.clone()));
    let observed = l.said();
    let replies: Vec<_> = observed
        .lines()
        .filter_map(|line| crate::jsondoc::parse(line).ok())
        .filter(|doc| doc.get("thumbToken").is_some())
        .collect();
    let legacy: Vec<_> = observed
        .lines()
        .filter(|line| {
            crate::jsondoc::parse(line)
                .unwrap()
                .get("thumbToken")
                .is_none()
        })
        .collect();
    assert_eq!(
        legacy,
        vec![thumbed_line(0, old_cache.to_str().unwrap(), 0.0)],
        "legacy completion stays byte compatible"
    );
    assert!(
        !replies.iter().any(
            |doc| doc.get("thumbToken").and_then(crate::jsondoc::Json::as_str) == Some("new")
                && doc.get("file").and_then(crate::jsondoc::Json::as_str) == old_cache.to_str()
        ),
        "predecessor mtime-10 work was authenticated as the mtime-20 request: {replies:?}"
    );
    assert_eq!(
        replies.len(),
        1,
        "current identity refusal must answer truthfully with no file"
    );
    assert_eq!(
        replies[0]
            .get("file")
            .and_then(crate::jsondoc::Json::as_str),
        Some("")
    );
}

#[test]
fn policy_identity_matching_path_with_wrong_listing_never_queues_or_replies() {
    let mut l = fixture("thumb-owner-listing-only");
    let path = crate::json::escape(&l.dir.join("old.jpg").to_string_lossy());
    let line = format!(
        r#"{{"c":"thumb","rows":[0],"thumbToken":"stale","thumbPath":"{path}","thumbListing":999}}"#
    );
    assert!(l.request(&line) == Control::Continue);
    assert_eq!(l.pool.pending(), 0);
    assert_eq!(l.st.outstanding, 0);
    assert!(l.st.asked.is_empty());
    assert!(l.said().is_empty());
}

#[test]
fn policy_identity_same_version_subscribers_publish_once_each_without_another_job() {
    let mut l = fixture("thumb-owner-eligible-live");
    ask(&mut l, "one", "old.jpg");
    ask(&mut l, "two", "old.jpg");
    ask(&mut l, "two", "old.jpg");
    assert_eq!(l.st.outstanding, 1);
    assert_eq!(l.pool.pending(), 1);
    let job = l.pool.cancel_all().pop().unwrap();
    let (context, path, mtime) = (job.replies.clone(), job.path.clone(), job.mtime);
    let raw = l.dir.join("actual-ready.png");
    crate::backend::thumbwrite::write_marker(
        &raw,
        &crate::backend::thumbcache::uri_for(&path),
        mtime,
        None,
    )
    .unwrap();
    finish(&mut l, job, Some(raw.clone()));
    let docs = tagged(&mut l);
    assert_eq!(docs.len(), 2);
    for token in ["one", "two"] {
        let owned: Vec<_> = docs
            .iter()
            .filter(|doc| {
                doc.get("thumbToken").and_then(crate::jsondoc::Json::as_str) == Some(token)
            })
            .collect();
        assert_eq!(owned.len(), 1);
        assert_eq!(
            owned[0].get("file").and_then(crate::jsondoc::Json::as_str),
            raw.to_str()
        );
    }
    report_done(
        &mut l.out,
        &mut l.st,
        &l.pool,
        Done {
            replies: context,
            path,
            mtime,
            trash_identity: None,
            result: Outcome::Ready(raw),
            ms: 0.0,
            trace: None,
        },
    );
    assert!(
        l.said().is_empty(),
        "duplicate completion cannot publish another terminal observation"
    );
    assert_eq!(l.st.outstanding, 0);
}
