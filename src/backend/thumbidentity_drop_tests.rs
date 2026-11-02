//! A returned job's observer owns its outcome even if an old legacy completion consumed its row map.
use super::*;
use crate::backend::{
    events::channel,
    listing::Listing,
    proto::{parse_request, Request},
    run::forget_rows,
    state::Tables,
    testdir::TestDir,
    thumbcache::Cache,
    thumbreq::{identity, report_done, thumb_rows},
    thumbs::{Done, Outcome, Pool},
    thumbspec::Thumbnailers,
};
use std::{io::BufWriter, sync::Arc};

#[test]
fn policy_identity_returned_orphan_delivers_and_flushes_its_captured_empty_outcome() {
    let dir = TestDir::new("thumb-identity-orphan-drop");
    let mut tables = Tables::load();
    tables.thumbs = Arc::new(Thumbnailers::from_entries(
        &[(
            "t.thumbnailer".into(),
            "[Thumbnailer Entry]\nExec=/usr/bin/false %i %o %s\nMimeType=image/jpeg;\n".into(),
        )],
        &tables.aliases,
    ))
    .into();
    let (events, _) = channel();
    let mut st = State::new(crate::backend::dirsizeworker::Worker::new(events));
    st.base = dir.path().to_path_buf();
    st.generation = 1;
    let mut listing = Listing::new();
    for row in 0..MAX_QUEUE + 2 {
        let name = format!("img-{row}.jpg");
        dir.file(&name, "never decoded by the idle pool");
        listing.push(&name, false);
    }
    st.listing = listing;
    let pool = Pool::idle();
    let cache = Cache::at(dir.join("cache"));
    let mut out = Vec::new();
    thumb_rows(&mut out, &[0], &mut st, &tables, &pool, &cache, false);
    let predecessor = pool.cancel_all().pop().unwrap();
    forget_rows(&mut st, &pool);
    let request = format!(
        r#"{{"c":"thumb","rows":[0],"thumbToken":"current","thumbPath":"{}","thumbListing":{}}}"#,
        crate::json::escape(&st.base.join("img-0.jpg").to_string_lossy()),
        st.generation
    );
    let Request::Thumb {
        rows,
        cache_only,
        identity,
    } = parse_request(&request)
    else {
        panic!("actual tagged request")
    };
    thumb_rows(
        &mut out,
        &rows,
        &mut st,
        &tables,
        &pool,
        &cache,
        identity::Options {
            cache_only,
            identity,
        },
    );
    let remaining: Vec<_> = (1..MAX_QUEUE + 2).collect();
    thumb_rows(&mut out, &remaining, &mut st, &tables, &pool, &cache, false);
    assert_eq!(pool.pending(), MAX_QUEUE);
    report_done(
        &mut out,
        &mut st,
        &pool,
        Done {
            replies: predecessor.replies,
            path: predecessor.path,
            mtime: predecessor.mtime,
            trash_identity: predecessor.trash_identity,
            result: Outcome::Failed,
            ms: 0.0,
            trace: predecessor.trace,
        },
    );
    assert!(
        st.asked.iter().all(|(_, row)| *row != 0),
        "actual predecessor consumed the legacy map"
    );
    // The real submit path can see a stale one-slot room snapshot after another submit filled the queue.
    let mut delivered = BufWriter::new(Vec::new());
    submit_waiting(&mut delivered, &mut st, &pool, 1);
    let text = std::str::from_utf8(delivered.get_ref()).unwrap();
    let tagged: Vec<_> = text
        .lines()
        .filter_map(|line| crate::jsondoc::parse(line).ok())
        .filter(|doc| doc.get("thumbToken").is_some())
        .collect();
    assert_eq!(
        tagged.len(),
        1,
        "immutable current observer was lost with mutable legacy mapping: {text}"
    );
    assert_eq!(
        tagged[0]
            .get("thumbToken")
            .and_then(crate::jsondoc::Json::as_str),
        Some("current")
    );
    assert_eq!(
        tagged[0].get("file").and_then(crate::jsondoc::Json::as_str),
        Some("")
    );
    assert_eq!(pool.pending(), MAX_QUEUE);
    assert_eq!(st.outstanding, MAX_QUEUE + 1);
}
