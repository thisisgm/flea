use super::*;
use crate::backend::aliases::Aliases;
use crate::backend::archive::Formats;
use crate::backend::events::channel;
use crate::backend::icons::Names;
use crate::backend::kind::Kinds;
use crate::backend::listing::Listing;
use crate::backend::mime::Db;
use crate::backend::testdir::TestDir;
use crate::backend::thumbcache;
use crate::backend::thumbspec::Thumbnailers;
use crate::backend::thumbwrite;
use std::cell::RefCell;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

// One jpeg declaration, so for_mime answers; the harness's pool has no worker, so the program it names never runs.
fn tables() -> Tables {
    let aliases = Arc::new(Aliases::load());
    let thumbs = Arc::new(Thumbnailers::from_entries(
        &[(
            "t.thumbnailer".to_string(),
            "[Thumbnailer Entry]\nExec=/usr/bin/false %i %o %s\nMimeType=image/jpeg;\n".to_string(),
        )],
        &aliases,
    ));
    assert!(
        thumbs.for_mime("image/jpeg", &aliases).is_some(),
        "the fixture declares jpeg"
    );
    Tables {
        mime: Arc::new(Db::load()).into(),
        icons: Arc::new(Names::load()).into(),
        aliases: aliases.into(),
        thumbs: thumbs.into(),
        kinds: RefCell::new(Kinds::new()),
        formats: Arc::new(Formats::probe()).into(),
    }
}

/// Sample input: rows [0] over a listing holding photo.jpg; out collects the wire.
fn listed(dir: &TestDir, st: &mut State) {
    st.base = dir.path().to_path_buf();
    let mut l = Listing::new();
    l.push("photo.jpg", false);
    st.listing = l;
    // The window statted this row, so a thumb request reuses its figures with no stat.
    if let Ok(m) = std::fs::symlink_metadata(dir.join("photo.jpg")) {
        st.window_meta.insert(0, (m.mode(), m.mtime(), false, 0));
    }
}

fn harness(dir: &TestDir) -> (State, Pool, Cache) {
    let (events, _) = channel();
    let st = State::new(crate::backend::dirsizeworker::Worker::new(events));
    let pool = Pool::idle();
    let cache = Cache::at(dir.join("cache"));
    (st, pool, cache)
}

#[test]
fn a_cache_only_miss_answers_none_and_queues_no_decode() {
    let d = TestDir::new("thumbcacheonly");
    d.file(
        "photo.jpg",
        "not a picture, so no decoder could read it either",
    );
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    listed(&d, &mut st);
    let mut out = Vec::new();
    thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, true);
    let line = String::from_utf8(out).unwrap();
    assert!(
        line.contains(r#""file":"""#),
        "a miss with no cache entry answers none: {}",
        line
    );
    assert_eq!(
        st.outstanding, 0,
        "no job may be in flight for a row that was answered"
    );
    assert!(
        st.asked.is_empty(),
        "and no mapping may hold it for a later report"
    );
    assert!(
        pool.cancel_all().is_empty(),
        "nothing reaches the pool for a cache-only miss"
    );
}

#[test]
fn a_cache_only_hit_serves_a_source_that_can_no_longer_be_opened() {
    let d = TestDir::new("thumbcacheonlyhit");
    let path = d.file("photo.jpg", "bytes the open is then refused for");
    let mtime = std::fs::metadata(&path).unwrap().mtime();
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    let uri = thumbcache::uri_for(&path);
    let large = cache.large_path(&uri);
    std::fs::create_dir_all(large.parent().unwrap()).unwrap();
    thumbwrite::write_marker(&large, &uri, mtime, None).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    listed(&d, &mut st);
    // The window already holds this row's figures, so the request reuses them with no stat.
    std::fs::remove_file(&path).unwrap();
    let mut out = Vec::new();
    thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, true);
    let line = String::from_utf8(out).unwrap();
    assert!(
        line.contains(&large.to_string_lossy().into_owned()),
        "the cached entry is served although the source no longer opens: {}",
        line
    );
    assert_eq!(st.outstanding, 0, "a served row queues nothing either");
    assert!(
        pool.cancel_all().is_empty(),
        "nothing reaches the pool for a served row"
    );
}

#[test]
fn a_windowed_row_queues_from_its_cached_figures_with_no_stat() {
    let d = TestDir::new("thumbcachedqueue");
    d.file("photo.jpg", "not a picture, and nothing here decodes it");
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    listed(&d, &mut st);
    std::fs::remove_file(d.join("photo.jpg")).unwrap();
    let mut out = Vec::new();
    thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, false);
    assert!(
        out.is_empty(),
        "a genuine miss is queued from cached figures, not answered: {}",
        String::from_utf8_lossy(&out)
    );
    assert_eq!(
        st.outstanding, 1,
        "the job is in flight from the window's figures alone"
    );
    assert_eq!(
        pool.cancel_all()
            .iter()
            .map(|j| j.path.clone())
            .collect::<Vec<_>>(),
        vec![d.join("photo.jpg")],
        "the queued job is the windowed row's own file"
    );
}

/// Verify that a link to a FIFO never reaches a decoder.
#[test]
fn a_symlink_to_a_fifo_is_refused_rather_than_queued() {
    // Sample input: link.jpg -> fifo lstats as S_IFLNK while the target is a fifo, so no job queues.
    extern "C" {
        fn mkfifo(path: *const std::ffi::c_char, mode: u32) -> std::ffi::c_int;
    }
    let d = TestDir::new("thumblinkfifo");
    let fifo = d.join("fifo.mp4");
    let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { mkfifo(c.as_ptr(), 0o644) },
        0,
        "fixture fifo must exist"
    );
    std::os::unix::fs::symlink(&fifo, d.join("link.jpg")).unwrap();
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    st.base = d.path().to_path_buf();
    let mut l = Listing::new();
    l.push("link.jpg", false);
    st.listing = l;
    if let Ok(m) = std::fs::symlink_metadata(d.join("link.jpg")) {
        assert_eq!(
            m.mode() & S_IFMT,
            S_IFLNK,
            "the window holds the link's lstat mode"
        );
        st.window_meta.insert(0, (m.mode(), m.mtime(), false, 0));
    }
    let mut out = Vec::new();
    thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, false);
    let line = String::from_utf8(out).unwrap();
    assert!(
        line.contains(r#""file":"""#),
        "a symlink to a fifo answers none: {}",
        line
    );
    assert_eq!(st.outstanding, 0, "no job may queue for a fifo target");
    assert!(st.asked.is_empty(), "and no mapping may hold it");
    assert!(
        pool.cancel_all().is_empty(),
        "nothing reaches the pool for a fifo target"
    );
}

/// Verify that changing a link target invalidates its cached thumbnail.
#[test]
fn a_symlink_row_keys_the_cache_on_its_target_mtime() {
    // Sample input: link.jpg -> photo.jpg with differing link and target mtimes queues the target's.
    let d = TestDir::new("thumblinkmtime");
    let target = d.file("photo.jpg", "not a picture, and nothing here decodes it");
    std::os::unix::fs::symlink(&target, d.join("link.jpg")).unwrap();
    // Backdate the target, so its mtime differs from the link's own fresh mtime.
    assert!(
        std::process::Command::new("touch")
            .arg("-d")
            .arg("2000-01-01 00:00:00")
            .arg(&target)
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
        "touch must backdate the target"
    );
    let target_mtime = std::fs::metadata(&target).unwrap().mtime();
    let link_mtime = std::fs::symlink_metadata(d.join("link.jpg"))
        .unwrap()
        .mtime();
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    st.base = d.path().to_path_buf();
    let mut l = Listing::new();
    l.push("link.jpg", false);
    st.listing = l;
    if let Ok(m) = std::fs::symlink_metadata(d.join("link.jpg")) {
        st.window_meta.insert(0, (m.mode(), m.mtime(), false, 0));
    }
    let mut out = Vec::new();
    thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, false);
    assert!(
        out.is_empty(),
        "a symlinked image is queued, not answered: {}",
        String::from_utf8_lossy(&out)
    );
    assert_eq!(st.outstanding, 1, "the target gates as a file and queues");
    let queued = pool.cancel_all();
    assert_eq!(queued.len(), 1, "one job queued");
    assert_eq!(
        queued[0].mtime, target_mtime,
        "the job keys the target mtime, not the link's {}",
        link_mtime
    );
    assert_ne!(
        queued[0].mtime, link_mtime,
        "link and target mtimes must differ for this pin"
    );
    for job in queued {
        forget_one(&mut st, &job.path);
    }
}

/// Verify that a cached target still serves a cache-only request.
#[test]
fn a_cached_symlink_is_served_under_cache_only() {
    // Sample input: link.jpg -> photo.jpg with a Ready entry keyed on the link path and target mtime.
    let d = TestDir::new("thumbcachesymlink");
    let target = d.file("photo.jpg", "bytes the link target holds");
    // Backdate the target below the link's fresh mtime so the two lookup keys cannot agree.
    const BACKDATED_SECS: u64 = 946684800;
    let past = std::time::UNIX_EPOCH + std::time::Duration::from_secs(BACKDATED_SECS);
    std::fs::File::options()
        .write(true)
        .open(&target)
        .unwrap()
        .set_modified(past)
        .unwrap();
    let link = d.join("link.jpg");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let target_mtime = std::fs::metadata(&target).unwrap().mtime();
    let link_mtime = std::fs::symlink_metadata(&link).unwrap().mtime();
    assert_ne!(
        target_mtime, link_mtime,
        "the mtimes must differ to tell the two keys apart"
    );
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    let uri = thumbcache::uri_for(&link);
    let large = cache.large_path(&uri);
    std::fs::create_dir_all(large.parent().unwrap()).unwrap();
    thumbwrite::write_marker(&large, &uri, target_mtime, None).unwrap();
    st.base = d.path().to_path_buf();
    let mut l = Listing::new();
    l.push("link.jpg", false);
    st.listing = l;
    if let Ok(m) = std::fs::symlink_metadata(&link) {
        st.window_meta.insert(0, (m.mode(), m.mtime(), false, 0));
    }
    let mut out = Vec::new();
    thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, true);
    let line = String::from_utf8(out).unwrap();
    assert!(
        line.contains(&large.to_string_lossy().into_owned()),
        "a cached symlink answers its entry under cacheOnly: {}",
        line
    );
    assert_eq!(st.outstanding, 0, "a served row queues nothing");
    assert!(
        pool.cancel_all().is_empty(),
        "nothing reaches the pool for a served symlink"
    );
}

#[test]
fn a_local_row_without_the_flag_takes_exactly_todays_path() {
    let d = TestDir::new("thumbfull");
    d.file("photo.jpg", "not a picture, and nothing here decodes it");
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    listed(&d, &mut st);
    let mut out = Vec::new();
    thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, false);
    assert!(
        out.is_empty(),
        "a genuine miss is queued, not answered: {}",
        String::from_utf8_lossy(&out)
    );
    assert_eq!(st.outstanding, 1, "the job is in flight as it always was");
    assert_eq!(st.asked.len(), 1, "and the mapping holds it for the report");
    assert_eq!(
        pool.cancel_all()
            .iter()
            .map(|j| j.path.clone())
            .collect::<Vec<_>>(),
        vec![d.join("photo.jpg")],
        "the queued job is the row's own file"
    );
}

// Sample input: a photo.jpg whose fail marker was written under marker_table, or under the loaded table when None; answers the wire, the jobs in flight and the jobs queued.
fn answer_with_marker(tag: &str, marker_table: Option<&str>) -> (String, usize, usize) {
    let d = TestDir::new(tag);
    let path = d.file("photo.jpg", "not a picture, and nothing here decodes it");
    let mtime = std::fs::metadata(&path).unwrap().mtime();
    let tb = tables();
    let (mut st, pool, cache) = harness(&d);
    let uri = thumbcache::uri_for(&path);
    let marker = cache.fail_path(&uri);
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    thumbwrite::write_marker(
        &marker,
        &uri,
        mtime,
        Some(marker_table.unwrap_or(tb.thumbs.fingerprint())),
    )
    .unwrap();
    listed(&d, &mut st);
    let mut out = Vec::new();
    thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, false);
    (
        String::from_utf8(out).unwrap(),
        st.outstanding,
        pool.cancel_all().len(),
    )
}

#[test]
fn a_failure_recorded_under_the_loaded_table_answers_none_and_queues_nothing() {
    let (line, outstanding, queued) = answer_with_marker("thumbfailsame", None);
    assert!(
        line.contains(r#""file":"""#),
        "a recorded failure answers none: {}",
        line
    );
    assert_eq!(
        (outstanding, queued),
        (0, 0),
        "and no decode is queued for it"
    );
}

#[test]
fn a_failure_recorded_under_another_table_is_tried_again() {
    let (line, outstanding, queued) = answer_with_marker("thumbfailother", Some("an older table"));
    assert!(
        line.is_empty(),
        "a marker from another table is a miss, queued and not answered: {}",
        line
    );
    assert_eq!((outstanding, queued), (1, 1), "the file is tried once more");
}
