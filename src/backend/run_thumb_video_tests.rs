//! Replacement accounting through the real thumb and thumbcancel request parser.
use super::*;
use crate::backend::{fdpass, thumbreq::identity, thumbs::Job};
use std::os::fd::AsRawFd;

fn cancelled(l: &mut Loop, job: Job) {
    report_done(
        &mut l.out,
        &mut l.st,
        &l.pool,
        Done {
            replies: job.replies,
            path: job.path,
            mtime: job.mtime,
            trash_identity: job.trash_identity,
            result: Outcome::Cancelled,
            ms: 0.0,
            trace: None,
        },
    );
}

#[test]
fn active_video_replacement_survives_both_completion_orders_and_repeated_cancels() {
    for old_first in [true, false] {
        let mut l = fixture("thumb-video-replacement");
        ask(&mut l, "old-a", "old.jpg");
        ask(&mut l, "old-b", "old.jpg");
        let old = l.pool.cancel_all().pop().unwrap();
        let (reply, worker) = fdpass::pair().unwrap();
        let reply = Arc::new(reply);
        identity::begin_video(old.replies.as_ref().unwrap());
        assert!(identity::arm_video(old.replies.as_ref().unwrap(), &reply));
        assert!(l.request(r#"{"c":"thumbcancel","rows":[0]}"#) == Control::Continue);
        assert!(l.request(r#"{"c":"thumbcancel","rows":[0]}"#) == Control::Continue);
        assert!(l.st.asked.is_empty());
        assert_eq!(l.st.outstanding, 1, "cancel does not release a live slot");
        assert_eq!(
            fdpass::recv(worker.as_raw_fd()).unwrap().unwrap().payload,
            [b'C']
        );
        assert!(identity::finish_video(old.replies.as_ref().unwrap()));
        ask(&mut l, "new-a", "old.jpg");
        ask(&mut l, "new-b", "old.jpg");
        assert_eq!(l.st.outstanding, 2);
        let replacement = l.pool.cancel_all().pop().unwrap();
        let file = l.dir.join("fresh.png");
        if old_first {
            cancelled(&mut l, old);
            assert!(l.out.is_empty(), "retired subscribers stay silent");
            assert_eq!(
                l.st.asked.len(),
                1,
                "old completion cannot unmap replacement"
            );
            finish(&mut l, replacement, Some(file.clone()));
        } else {
            finish(&mut l, replacement, Some(file.clone()));
            cancelled(&mut l, old);
        }
        assert_eq!(l.st.outstanding, 0);
        assert!(l.st.asked.is_empty());
        let docs = tagged(&mut l);
        assert_eq!(docs.len(), 2);
        for (doc, token) in docs.iter().zip(["new-a", "new-b"]) {
            assert_eq!(
                doc.get("thumbToken").and_then(crate::jsondoc::Json::as_str),
                Some(token)
            );
            assert_eq!(
                doc.get("file").and_then(crate::jsondoc::Json::as_str),
                file.to_str()
            );
        }
    }
}

#[test]
fn cancelling_one_active_video_does_not_retire_another_rows_subscriber() {
    let mut l = fixture("thumb-video-other-row");
    l.st.listing.push("new.jpg", false);
    ask(&mut l, "old", "old.jpg");
    let old = l.pool.cancel_all().pop().unwrap();
    identity::begin_video(old.replies.as_ref().unwrap());
    let path = l.dir.join("new.jpg");
    let request = format!(
        r#"{{"c":"thumb","rows":[1],"thumbToken":"other","thumbPath":"{}","thumbListing":1}}"#,
        crate::json::escape(&path.to_string_lossy())
    );
    assert!(l.request(&request) == Control::Continue);
    let other = l.pool.cancel_all().pop().unwrap();
    assert!(l.request(r#"{"c":"thumbcancel","rows":[0]}"#) == Control::Continue);
    assert!(identity::video_cancelled(old.replies.as_ref().unwrap()));
    let file = l.dir.join("other.png");
    finish(&mut l, other, Some(file.clone()));
    cancelled(&mut l, old);
    assert_eq!(l.st.outstanding, 0);
    let docs = tagged(&mut l);
    assert_eq!(docs.len(), 1);
    assert_eq!(
        docs[0]
            .get("thumbToken")
            .and_then(crate::jsondoc::Json::as_str),
        Some("other")
    );
    assert_eq!(
        docs[0].get("file").and_then(crate::jsondoc::Json::as_str),
        file.to_str()
    );
}
