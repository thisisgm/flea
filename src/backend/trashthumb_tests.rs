use super::*;
use crate::backend::testdir::TestDir;
use crate::backend::thumbreq::{cancel_all, report_done};
use crate::backend::thumbs::{Done, Outcome};
use crate::backend::thumbspec::Thumbnailers;
use crate::backend::events::channel;
use std::sync::Arc;

fn request_line(path: &Path, id: usize) -> String {
    use std::os::unix::fs::MetadataExt;
    let meta = path.symlink_metadata().unwrap();
    let provider = format!("l{}:{}", meta.dev(), meta.ino());
    let reviewed = Reviewed::inspect(path.to_path_buf(), &provider).unwrap();
    format!(r#"{{"c":"trashthumb","id":{},"original":"/original/photo.jpg","path":"{}","provider":"{}","identity":"{}"}}"#,
        id, escape(&path.to_string_lossy()), provider, escape(&reviewed.selection_identity()))
}

fn fixture(d: &TestDir) -> (Tables, State, Pool, Cache, String, PathBuf, i64) {
    let mut tb = Tables::load();
    tb.thumbs = Arc::new(Thumbnailers::from_entries(&[("test.thumbnailer".into(),
        "[Thumbnailer Entry]\nExec=/usr/bin/false %i %o %s\nMimeType=image/jpeg;\n".into())], &tb.aliases)).into();
    let (events, _) = channel();
    let st = State::new(crate::backend::dirsizeworker::Worker::new(events));
    d.dir("files");
    d.dir("info");
    let path = d.file("files/photo (1).jpg", "image bytes");
    d.file("info/photo (1).jpg.trashinfo", "[Trash Info]\nPath=/original/photo.jpg\n");
    use std::os::unix::fs::MetadataExt;
    let meta = path.symlink_metadata().unwrap();
    let line = request_line(&path, 1);
    (tb, st, Pool::idle(), Cache::at(d.join("cache")), line, path, meta.mtime())
}

#[test]
fn trash_thumb_uses_shared_pool_once_and_the_backing_uri_cache_key() {
    let d = TestDir::new("trash-thumb-queue");
    let (tb, mut st, pool, cache, line, path, mtime) = fixture(&d);
    let mut out = Vec::new();
    request(&mut out, &mut st, &tb, &pool, &cache, &line);
    request(&mut out, &mut st, &tb, &pool, &cache, &request_line(&path, 2));
    assert_eq!((pool.pending(), st.outstanding, st.trash_asked.len()), (1, 1, 2));
    assert!(out.is_empty());
    let file = cache.large_path_for(&crate::backend::thumbcache::uri_for(&path), field_str(&line, "identity").as_deref());
    std::fs::create_dir_all(cache.large_dir()).unwrap();
    crate::backend::thumbwrite::write_marker(&file, &crate::backend::thumbcache::uri_for(&path), mtime, None).unwrap();
    let job = pool.cancel_all().pop().unwrap();
    report_done(&mut out, &mut st, &pool, Done { replies: None, path, mtime, trash_identity: job.trash_identity, result: Outcome::Ready(file.clone()), ms: 0.0, trace: None });
    let replies = String::from_utf8(out).unwrap();
    assert_eq!(replies.lines().map(|line| field_usize(line, "id").unwrap()).collect::<Vec<_>>(), vec![1, 2]);
    assert!(replies.lines().all(|line| field_str(line, "file") == Some(file.to_string_lossy().into_owned())));
    assert!(st.trash_asked.is_empty());
    let mut warm = Vec::new();
    request(&mut warm, &mut st, &tb, &pool, &cache, &line);
    assert_eq!(field_str(&String::from_utf8(warm).unwrap(), "file"), Some(file.to_string_lossy().into_owned()));
    assert_eq!(st.outstanding, 0);
}

#[test]
fn trashthumb_replacement_while_the_predecessor_runs_queues_and_delivers_its_own_job() {
    use std::os::unix::fs::MetadataExt;
    for listing in [false, true] {
        for newest_first in [false, true] {
            let d = TestDir::new("trash-thumb-inflight-replacement");
            let (tb, mut st, pool, cache, line, path, mtime) = fixture(&d);
            let mut out = Vec::new();
            if listing {
                st.base = d.join("files");
                st.listing.push("photo (1).jpg", false);
                st.window_meta.insert(0, (path.symlink_metadata().unwrap().mode(), mtime, false, 0));
                crate::backend::thumbreq::thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, false);
            } else { request(&mut out, &mut st, &tb, &pool, &cache, &line); }
            let running = pool.cancel_all().pop().unwrap();
            assert_eq!(running.mtime, mtime, "the predecessor has left the queue and not reported");
            d.replace_file(&path, "replacement image bytes");
            d.assert_contains(&path);
            let modified = std::time::UNIX_EPOCH + std::time::Duration::from_secs((mtime + 2) as u64);
            std::fs::File::options().write(true).open(&path).unwrap().set_modified(modified).unwrap();
            let replacement_mtime = path.symlink_metadata().unwrap().mtime();
            assert_ne!(replacement_mtime, mtime);
            if listing { st.window_meta.insert(0, (path.symlink_metadata().unwrap().mode(), replacement_mtime, false, 0)); }
            request(&mut out, &mut st, &tb, &pool, &cache, &request_line(&path, 2));
            assert_eq!((pool.pending(), st.outstanding), (1, 2), "replacement queues its own job while predecessor runs, listing={listing}");
            let queued = pool.cancel_all().pop().unwrap();
            assert_eq!(queued.mtime, replacement_mtime);
            let old = Done { replies: None, path: path.clone(), mtime, trash_identity: running.trash_identity, result: Outcome::Ready(d.join("predecessor.png")), ms: 0.0, trace: None };
            let new = Done { replies: None, path: path.clone(), mtime: replacement_mtime, trash_identity: queued.trash_identity, result: Outcome::Ready(d.join("replacement.png")), ms: 0.0, trace: None };
            let (first, second) = if newest_first { (new, old) } else { (old, new) };
            report_done(&mut out, &mut st, &pool, first);
            assert_eq!(st.trash_asked.iter().any(|ask| ask.id == 2), !newest_first, "only the matching completion consumes the new subscriber");
            report_done(&mut out, &mut st, &pool, second);
            let replies = String::from_utf8(out).unwrap();
            let replacement: Vec<_> = replies.lines().filter(|line| field_str(line, "t").as_deref() == Some("trashthumbed") && field_usize(line, "id") == Some(2)).collect();
            assert_eq!(replacement.len(), 1, "the replacement gets exactly one reply");
            assert_eq!(field_str(replacement[0], "file"), Some(d.join("replacement.png").to_string_lossy().into_owned()));
            assert!(st.trash_asked.is_empty());
            assert_eq!(st.outstanding, 0);
        }
    }
}

fn same_mtime_replacement(failed: bool) {
    use std::os::unix::fs::MetadataExt;
    for newest_first in [false, true] {
        let d = TestDir::new("trash-thumb-same-mtime");
        let (tb, mut st, pool, cache, _, path, mtime) = fixture(&d);
        let mut out = Vec::new();
        d.assert_contains(&path);
        let modified = std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime as u64);
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(modified).unwrap();
        request(&mut out, &mut st, &tb, &pool, &cache, &request_line(&path, 1));
        let running = pool.cancel_all().pop().unwrap();
        let old_meta = path.symlink_metadata().unwrap();
        d.replace_file(&path, "other bytes");
        d.assert_contains(&path);
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(modified).unwrap();
        let replacement = path.symlink_metadata().unwrap();
        assert_ne!(replacement.ino(), old_meta.ino());
        assert_eq!((replacement.len(), replacement.mtime_nsec(), replacement.mode()), (old_meta.len(), old_meta.mtime_nsec(), old_meta.mode()));
        assert_eq!(replacement.mtime(), running.mtime, "both jobs have the same integer mtime");
        request(&mut out, &mut st, &tb, &pool, &cache, &request_line(&path, 2));
        assert_eq!((pool.pending(), st.outstanding), (1, 2), "same-mtime replacement queues independently, failed={failed}");
        let queued = pool.cancel_all().pop().unwrap();
        let result = if failed { Outcome::Failed } else { Outcome::Ready(d.join("predecessor.png")) };
        let old = Done { replies: None, path: running.path, mtime: running.mtime, trash_identity: running.trash_identity, result, ms: 0.0, trace: None };
        let new = Done { replies: None, path: queued.path, mtime: queued.mtime, trash_identity: queued.trash_identity, result: Outcome::Ready(d.join("replacement.png")), ms: 0.0, trace: None };
        let (first, second) = if newest_first { (new, old) } else { (old, new) };
        report_done(&mut out, &mut st, &pool, first);
        assert_eq!(st.trash_asked.iter().any(|ask| ask.id == 2), !newest_first, "only the replacement's own completion consumes its subscriber");
        report_done(&mut out, &mut st, &pool, second);
        let replies = String::from_utf8(out).unwrap();
        let own: Vec<_> = replies.lines().filter(|line| field_usize(line, "id") == Some(2)).collect();
        assert_eq!(own.len(), 1);
        assert_eq!(field_str(own[0], "file"), Some(d.join("replacement.png").to_string_lossy().into_owned()));
        assert!(st.trash_asked.is_empty());
        assert_eq!(st.outstanding, 0);
    }
}

#[test]
fn trashthumb_same_mtime_new_inode_never_receives_the_predecessors_picture() {
    same_mtime_replacement(false);
}

#[test]
fn trashthumb_same_mtime_valid_replacement_is_not_consumed_by_a_failed_predecessor() {
    same_mtime_replacement(true);
}

#[test]
fn trash_thumb_refuses_vanished_replaced_and_non_image_backings() {
    for case in ["missing", "replaced", "text"] {
        let d = TestDir::new(&format!("trash-thumb-{case}"));
        let (tb, mut st, pool, cache, mut line, path, _) = fixture(&d);
        if case == "text" { line = line.replace("photo.jpg", "notes.txt"); }
        else {
            assert!(path.is_absolute() && path.starts_with(d.path()) && path != d.path());
            std::fs::rename(&path, d.join("files/saved.jpg")).unwrap();
            if case == "replaced" { std::fs::write(&path, "replacement").unwrap(); }
        }
        let mut out = Vec::new();
        request(&mut out, &mut st, &tb, &pool, &cache, &line);
        assert_eq!(pool.pending(), 0, "{case} never reaches the decoder queue");
        assert_eq!(field_str(&String::from_utf8(out).unwrap(), "file"), Some(String::new()));
    }
}

#[test]
fn trash_thumb_cancellation_and_replacement_drop_late_images() {
    for cancelled in [false, true] {
        let d = TestDir::new("trash-thumb-late");
        let (tb, mut st, pool, cache, line, path, mtime) = fixture(&d);
        let mut out = Vec::new();
        request(&mut out, &mut st, &tb, &pool, &cache, &line);
        let job = pool.cancel_all().pop().unwrap();
        if cancelled { cancel(&mut out, &mut st, &pool, &[1]); }
        else {
            assert!(path.is_absolute() && path.starts_with(d.path()) && path != d.path());
            std::fs::rename(&path, d.join("files/reviewed.jpg")).unwrap();
            std::fs::write(&path, "replacement").unwrap();
        }
        report_done(&mut out, &mut st, &pool, Done { replies: None, path, mtime, trash_identity: job.trash_identity, result: Outcome::Ready(d.join("cached.png")), ms: 0.0, trace: None });
        if cancelled { assert!(out.is_empty(), "cancelled running work emits nothing"); }
        else { assert_eq!(field_str(&String::from_utf8(out).unwrap(), "file"), Some(String::new())); }
        assert_eq!(st.outstanding, 0);
    }
}

#[test]
fn ordinary_listing_cancellation_keeps_trash_jobs_and_trash_cancel_keeps_listing_jobs() {
    let d = TestDir::new("trash-thumb-shared");
    let (tb, mut st, pool, cache, line, path, mtime) = fixture(&d);
    use std::os::unix::fs::MetadataExt;
    st.base = d.join("files");
    st.listing.push("photo (1).jpg", false);
    st.window_meta.insert(0, (path.symlink_metadata().unwrap().mode(), mtime, false, 0));
    let mut out = Vec::new();
    request(&mut out, &mut st, &tb, &pool, &cache, &line);
    cancel_all(&mut st, &pool);
    assert_eq!(pool.pending(), 1);
    crate::backend::thumbreq::thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, false);
    assert_eq!(pool.pending(), 2, "unknown listing identity does not join the Trash job");
    cancel_all(&mut st, &pool);
    assert!(st.asked.is_empty(), "listing cancellation removes only its subscriber");
    assert_eq!(pool.pending(), 1);
    crate::backend::thumbreq::thumb_rows(&mut out, &[0], &mut st, &tb, &pool, &cache, false);
    cancel(&mut out, &mut st, &pool, &[1]);
    assert_eq!(pool.pending(), 1);
    cancel_all(&mut st, &pool);
    assert_eq!(pool.pending(), 0, "no owner still wants the queued job");
    assert!(st.trash_asked.is_empty(), "a cancelled mapping cannot swallow the next ask");
}

#[test]
fn a_trash_cache_only_miss_never_queues_a_decoder() {
    let d = TestDir::new("trash-thumb-cache-only");
    let (tb, mut st, pool, cache, line, _, _) = fixture(&d);
    let line = line.replacen('{', "{\"cacheOnly\":true,", 1);
    let mut out = Vec::new();
    request(&mut out, &mut st, &tb, &pool, &cache, &line);
    assert_eq!(pool.pending(), 0);
    assert_eq!(st.outstanding, 0);
    assert_eq!(field_str(&String::from_utf8(out).unwrap(), "file"), Some(String::new()));
}

// Generated with Python zlib and CRC checked: real 1x1 grayscale black and white PNGs.
const CACHE_BLACK: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x00\x00\x00\x00\x3a\x7e\x9b\x55\x00\x00\x00\x0a\x49\x44\x41\x54\x78\xda\x63\x60\x00\x00\x00\x02\x00\x01\xe5\x27\xde\xfc\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";
const CACHE_WHITE: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x00\x00\x00\x00\x3a\x7e\x9b\x55\x00\x00\x00\x0a\x49\x44\x41\x54\x78\xda\x63\xf8\x0f\x00\x01\x01\x01\x00\x1c\xb0\x8c\x99\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";

fn same_stamp_cache_backing(d: &TestDir, path: &Path, mtime: i64) -> String {
    use std::os::unix::fs::MetadataExt;
    let old_inode = path.symlink_metadata().unwrap().ino();
    d.replace_file(path, "other bytes");
    d.assert_contains(path);
    let modified = std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime as u64);
    std::fs::File::options().write(true).open(path).unwrap().set_modified(modified).unwrap();
    assert_ne!(path.symlink_metadata().unwrap().ino(), old_inode);
    assert_eq!(path.symlink_metadata().unwrap().mtime(), mtime);
    request_line(path, 2)
}

fn cache_png(d: &TestDir, cache: &Cache, uri: &str, mtime: i64, file: PathBuf, png: &[u8]) -> PathBuf {
    std::fs::create_dir_all(cache.large_dir()).unwrap();
    let temp = crate::backend::thumbwrite::exclusive_temp(&cache.large_dir()).unwrap();
    d.assert_contains(&temp);
    d.assert_contains(&file);
    std::fs::write(&temp, png).unwrap();
    let Outcome::Ready(ready) = crate::backend::thumbs::publish(&temp, uri, mtime, file) else { panic!("valid PNG did not publish"); };
    let bytes = std::fs::read(&ready).unwrap();
    assert_eq!(crate::backend::thumbcache::png_text(&bytes, "Thumb::URI"), Some(uri.into()));
    assert_eq!(crate::backend::thumbcache::png_text(&bytes, "Thumb::MTime"), Some(mtime.to_string()));
    ready
}

fn old_shared_cache_for_new_trash_identity(failed: bool) {
    let d = TestDir::new("trash-shared-cache-identity");
    let (tb, mut st, pool, cache, line, path, mtime) = fixture(&d);
    let uri = crate::backend::thumbcache::uri_for(&path);
    if failed { crate::backend::thumbs::record_failure(&cache, &uri, mtime, tb.thumbs.fingerprint()); }
    else { cache_png(&d, &cache, &uri, mtime, cache.large_path(&uri), CACHE_BLACK); }
    let new_line = same_stamp_cache_backing(&d, &path, mtime);
    assert_ne!(field_str(&new_line, "identity"), field_str(&line, "identity"));
    assert!(inspect(&new_line, &tb).is_some(), "the replacement's Reviewed identity is valid");
    assert!(if failed { matches!(cache.lookup(&path, mtime, tb.thumbs.fingerprint()), Hit::Failed) }
        else { matches!(cache.lookup(&path, mtime, tb.thumbs.fingerprint()), Hit::Ready(_)) }, "ordinary URI+mtime cache behavior remains deliberate");
    let mut out = Vec::new();
    request(&mut out, &mut st, &tb, &pool, &cache, &new_line);
    assert_eq!(pool.pending(), 1, "a cache entry with unknown reviewed identity cannot answer the new Trash backing, failed={failed}: {}", String::from_utf8_lossy(&out));
    assert!(out.is_empty());
}

#[test]
fn trashthumb_cache_ready_cannot_cross_a_same_stamp_inode_replacement() {
    old_shared_cache_for_new_trash_identity(false);
}

#[test]
fn trashthumb_cache_failed_cannot_cross_a_same_stamp_inode_replacement() {
    old_shared_cache_for_new_trash_identity(true);
}

// Sample input: PNG signature followed by big-endian length, four-byte chunk type (IDAT), payload and four-byte CRC.
fn png_pixels(bytes: &[u8]) -> &[u8] {
    let mut at = 8;
    while at + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        if &bytes[at + 4..at + 8] == b"IDAT" { return &bytes[at + 8..at + 8 + len]; }
        at += 12 + len;
    }
    panic!("valid cached PNG has no pixels");
}

#[test]
fn trashthumb_producer_ready_publications_do_not_overwrite_the_current_picture() {
    if crate::backend::sandboxprobe::skipped() { return; }
    for newest_first in [true, false] {
        let d = TestDir::new("trash-cache-publish-order");
        let (mut tb, _, _, cache, line, path, mtime) = fixture(&d);
        let old_identity = field_str(&line, "identity").unwrap();
        let new_line = same_stamp_cache_backing(&d, &path, mtime);
        let new_identity = field_str(&new_line, "identity").unwrap();
        assert_ne!(old_identity, new_identity);
        let uri = crate::backend::thumbcache::uri_for(&path);
        let old_file = cache.large_path_for(&uri, Some(&old_identity));
        let new_file = cache.large_path_for(&uri, Some(&new_identity));
        producer_publication(&d, &mut tb, &path, mtime, None, Some(CACHE_BLACK));
        let ordinary = std::fs::read(cache.large_path(&uri)).unwrap();
        let order = if newest_first { [(&new_identity, CACHE_WHITE), (&old_identity, CACHE_BLACK)] }
            else { [(&old_identity, CACHE_BLACK), (&new_identity, CACHE_WHITE)] };
        for (identity, png) in order {
            producer_publication(&d, &mut tb, &path, mtime, Some(identity), Some(png));
        }
        let bytes = std::fs::read(&new_file).unwrap();
        assert_eq!(png_pixels(&bytes), png_pixels(CACHE_WHITE), "the current cache entry must keep its own picture when newest_first={newest_first}");
        assert_ne!(old_file, new_file, "each reviewed producer needs a stable output path");
        assert!(matches!(cache.lookup_for(&path, mtime, tb.thumbs.fingerprint(), Some(&new_identity)), Hit::Ready(ref file) if *file == new_file));
        assert_eq!(png_pixels(&std::fs::read(&old_file).unwrap()), png_pixels(CACHE_BLACK));
        assert_eq!(std::fs::read(cache.large_path(&uri)).unwrap(), ordinary, "Trash must not overwrite an ordinary thumbnail");
        assert!(matches!(cache.lookup(&path, mtime, tb.thumbs.fingerprint()), Hit::Ready(ref file) if *file == cache.large_path(&uri)));
    }
}

// The helper supplies captured decoder pixels, including an old job's bytes after replacement; publication is always production run_one_in.
fn producer_publication(d: &TestDir, tb: &mut Tables, path: &Path, mtime: i64, identity: Option<&str>, png: Option<&[u8]>) {
    let body = png.map_or_else(|| "#!/bin/sh\nexit 1\n".into(), |png| {
        let octal: String = png.iter().map(|byte| format!("\\{byte:03o}")).collect();
        format!("#!/bin/sh\nprintf '{octal}' > \"$2\"\n")
    });
    let decoder = d.script("trash-producer", &body);
    let entry = format!("[Thumbnailer Entry]\nExec={} %i %o\nMimeType=image/jpeg;\n", decoder.display());
    tb.thumbs = Arc::new(Thumbnailers::from_entries(&[("producer.thumbnailer".into(), entry)], &tb.aliases)).into();
    let tables = crate::backend::thumbs::Tables { aliases: Arc::clone(&tb.aliases), specs: Arc::clone(&tb.thumbs), cache: Cache::at(d.join("cache")), worker: None };
    let mut job = Job { replies: None, path: path.into(), mtime, source_size: None, trash_identity: identity.map(str::to_owned), mime: "image/jpeg".into(), trace: None };
    let result = crate::backend::thumbs::run_one_in(&tables, &mut job, &d.join("raw"));
    assert_eq!(job.trash_identity.as_deref(), identity);
    let uri = crate::backend::thumbcache::uri_for(path);
    let file = match (result, png) {
        (Outcome::Ready(file), Some(_)) => { assert_eq!(file, tables.cache.large_path_for(&uri, identity)); file }
        (Outcome::Failed, None) => {
            assert!(matches!(tables.cache.lookup_for(path, mtime, tb.thumbs.fingerprint(), identity), Hit::Failed), "a decoder verdict must record its owner's marker");
            tables.cache.fail_path_for(&uri, identity)
        }
        _ => panic!("the real producer did not return the decoder's expected verdict"),
    };
    let bytes = std::fs::read(file).unwrap();
    assert_eq!(crate::backend::thumbcache::png_text(&bytes, "Thumb::URI"), Some(uri));
    assert_eq!(crate::backend::thumbcache::png_text(&bytes, "Thumb::MTime"), Some(mtime.to_string()));
}

#[test]
fn trashthumb_producer_failed_and_ready_publications_keep_their_reviewed_owners() {
    if crate::backend::sandboxprobe::skipped() { return; }
    for old_failed in [false, true] {
        for newest_first in [false, true] {
            let d = TestDir::new("trash-cache-marker-order");
            let (mut tb, mut st, pool, cache, line, path, mtime) = fixture(&d);
            let old_identity = field_str(&line, "identity").unwrap();
            let new_line = same_stamp_cache_backing(&d, &path, mtime);
            let new_identity = field_str(&new_line, "identity").unwrap();
            let uri = crate::backend::thumbcache::uri_for(&path);
            let new_file = cache.large_path_for(&uri, Some(&new_identity));
            producer_publication(&d, &mut tb, &path, mtime, None, None);
            let ordinary = std::fs::read(cache.fail_path(&uri)).unwrap();
            let old_png = if old_failed { None } else { Some(CACHE_BLACK) };
            let new_png = if old_failed { Some(CACHE_WHITE) } else { None };
            let order = if newest_first { [(&new_identity, new_png), (&old_identity, old_png)] }
                else { [(&old_identity, old_png), (&new_identity, new_png)] };
            for (identity, png) in order { producer_publication(&d, &mut tb, &path, mtime, Some(identity), png); }
            let mut out = Vec::new();
            request(&mut out, &mut st, &tb, &pool, &cache, &new_line);
            let reply = String::from_utf8(out).unwrap();
            assert_eq!(pool.pending(), 0, "the current reviewed cache result is warm");
            assert_eq!(field_str(&reply, "file"), Some(if old_failed { new_file.to_string_lossy().into_owned() } else { String::new() }));
            assert_ne!(cache.fail_path_for(&uri, Some(&old_identity)), cache.fail_path_for(&uri, Some(&new_identity)));
            if old_failed { assert_eq!(png_pixels(&std::fs::read(&new_file).unwrap()), png_pixels(CACHE_WHITE)); }
            let table_miss = cache.lookup_for(&path, mtime, "different-table", Some(&new_identity));
            assert!(if old_failed { matches!(table_miss, Hit::Ready(_)) } else { matches!(table_miss, Hit::Miss) });
            assert_eq!(std::fs::read(cache.fail_path(&uri)).unwrap(), ordinary, "Trash must not overwrite an ordinary failure marker");
            assert!(matches!(cache.lookup(&path, mtime, tb.thumbs.fingerprint()), Hit::Failed));
        }
    }
}

#[test]
fn trashthumb_cache_only_does_not_decode_an_unknown_shared_cache_identity() {
    let d = TestDir::new("trash-cache-only-unknown");
    let (tb, mut st, pool, cache, line, path, mtime) = fixture(&d);
    let uri = crate::backend::thumbcache::uri_for(&path);
    cache_png(&d, &cache, &uri, mtime, cache.large_path(&uri), CACHE_BLACK);
    let line = line.replacen('{', "{\"cacheOnly\":true,", 1);
    let mut out = Vec::new();
    request(&mut out, &mut st, &tb, &pool, &cache, &line);
    assert_eq!(pool.pending(), 0);
    assert_eq!(field_str(&String::from_utf8(out).unwrap(), "file"), Some(String::new()));
}
