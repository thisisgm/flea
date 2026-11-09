use super::*;
use crate::backend::{aliases::Aliases, icons::Names, kind::Kinds, mime::Db, rows, tags, testdir::TestDir, thumbspec::Thumbnailers};
use crate::backend::watch::{IN_IGNORED, IN_Q_OVERFLOW, IN_UNMOUNT};
use std::os::unix::fs::{symlink, PermissionsExt};

fn listing(names: &[&str]) -> Listing {
    let mut listing = Listing::new();
    for name in names { listing.push(name, false); }
    listing
}

fn counts() -> (usize, usize) {
    (super::super::meta::SEARCH_STAT_CALLS.with(|calls| calls.replace(0)), tags::SEARCH_TAG_CALLS.with(|calls| calls.replace(0)))
}

fn render(cache: &mut Cache, base: &Path, listing: &Listing, start: usize, count: usize, body: &str) -> String {
    let (metas, _) = cache.window(base, listing, start, count, body);
    let aliases = Aliases::load();
    let thumbs = Thumbnailers::load(&aliases);
    rows::rows_line_cached(base, body, listing, &metas, start, 0.0, &Db::load(), &Names::load(), &aliases, &thumbs, &mut Kinds::new(), cache)
}

#[test]
fn search_row_cache_stats_each_row_and_reuses_only_fresh_intersecting_tags() {
    let d = TestDir::new("search-row-counts");
    for name in ["a.txt", "b.txt", "c.txt", "d.txt"] { d.file(name, name); }
    let l = listing(&["a.txt", "b.txt", "c.txt", "d.txt"]);
    let mut cache = Cache::default();
    counts();
    render(&mut cache, d.path(), &l, 0, 3, "");
    assert_eq!(counts(), (6, 3));
    render(&mut cache, d.path(), &l, 1, 3, "");
    assert_eq!(counts(), (4, 1), "two carried identities validate metadata and reuse tags");
    assert_eq!(cache.entries.len(), 3);
    assert!(!cache.entries.contains_key(&d.join("a.txt")), "scrolled-past identity is discarded");
    render(&mut cache, d.path(), &l, 0, 3, "");
    assert_eq!(counts(), (4, 1), "returning to an evicted identity is a cold miss");
}

#[test]
fn search_row_cache_keeps_a_raw_named_row_under_its_own_path() {
    use flea::bytename::ExactText as _;
    use std::os::unix::ffi::OsStrExt;
    let d = TestDir::new("search-row-raw");
    let raw = std::ffi::OsStr::from_bytes(b"caf\xe9.txt");
    std::fs::write(d.path().join(raw), "raw").unwrap();
    let mut cache = Cache::default();
    render(
        &mut cache,
        d.path(),
        &listing(&[&raw.exact_text()]),
        0,
        1,
        "",
    );
    assert!(
        cache.entries.contains_key(&d.path().join(raw)),
        "the raw row is cached at the file it names"
    );
}

#[test]
fn search_row_cache_reindexes_kinds_and_preserves_every_fresh_row_field() {
    let d = TestDir::new("search-row-kinds");
    d.file("a.txt", "abc");
    d.file("b.png", "image");
    tags::restore(&d.join("a.txt"), &Some(b"Red,Blue".to_vec())).unwrap();
    let body = super::super::mountinfo::read_body();
    let mut cache = Cache::default();
    render(&mut cache, d.path(), &listing(&["a.txt", "b.png"]), 0, 2, &body);
    let reversed = listing(&["b.png", "a.txt"]);
    let warm = render(&mut cache, d.path(), &reversed, 0, 2, &body);
    let fresh = render(&mut Cache::default(), d.path(), &reversed, 0, 2, &body);
    assert_eq!(warm, fresh);
    assert!(warm.contains(r#""tg":["Red","Blue"]"#));
}

#[test]
fn search_row_cache_invalidates_size_mode_mtime_tags_replacement_and_ancestor_moves() {
    let d = TestDir::new("search-row-invalidate");
    d.dir("parent/child");
    let file = d.file("parent/child/a.txt", "a");
    let l = listing(&["parent/child/a.txt"]);
    let mut cache = Cache::default();
    render(&mut cache, d.path(), &l, 0, 1, "");
    for mutation in 0..5 {
        match mutation {
            0 => std::fs::write(&file, "more bytes").unwrap(),
            1 => std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap(),
            2 => std::fs::File::options().write(true).open(&file).unwrap().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(12345)).unwrap(),
            3 => tags::restore(&file, &Some(b"Green".to_vec())).unwrap(),
            _ => d.replace_file(&file, "replacement"),
        }
        let before = cache.misses;
        let cached = render(&mut cache, d.path(), &l, 0, 1, "");
        let fresh = render(&mut Cache::default(), d.path(), &l, 0, 1, "");
        assert_eq!(cached, fresh, "mutation {mutation}");
        assert_eq!(cache.misses, before + 1);
    }
    d.assert_contains(&d.join("parent"));
    d.assert_contains(&d.join("moved"));
    std::fs::rename(d.join("parent"), d.join("moved")).unwrap();
    let (metas, _) = cache.window(d.path(), &l, 0, 1, "");
    assert_eq!(metas[0].mode, 0, "moving an ancestor cannot retain the old path's facts");
}

#[test]
fn search_row_cache_bypasses_symlinks_unwatched_remotes_and_failed_watches() {
    let d = TestDir::new("search-row-bypass");
    d.dir("folder");
    d.file("file.txt", "a");
    symlink("file.txt", d.join("link")).unwrap();
    let l = listing(&["link"]);
    let mut cache = Cache::default();
    assert!(!cache.window(d.path(), &l, 0, 1, "").0[0].target_is_dir);
    d.assert_contains(&d.join("link"));
    std::fs::remove_file(d.join("link")).unwrap();
    symlink("folder", d.join("link")).unwrap();
    assert!(cache.window(d.path(), &l, 0, 1, "").0[0].target_is_dir);
    assert!(cache.entries.is_empty());
    let body = format!("1 0 0:45 / {} rw - nfs host:/share rw\n", d.path().display());
    let l = listing(&["file.txt"]);
    cache.window(d.path(), &l, 0, 1, &body);
    cache.window(d.path(), &l, 0, 1, &body);
    assert!(cache.entries.is_empty());
    let missing = listing(&["absent/file.txt"]);
    cache.window(d.path(), &missing, 0, 1, "");
    assert!(cache.entries.is_empty());
    // A failed watch removes all active coverage and facts while retaining descriptors for the next request.
    super::test_support::assert_cleared(&cache);
}

#[test]
fn search_row_cache_is_bounded_and_clears_all_session_state() {
    let d = TestDir::new("search-row-bound");
    let mut l = Listing::new();
    for i in 0..500 { let name = format!("a{i}.txt"); d.file(&name, "a"); l.push(&name, false); }
    let mut cache = Cache::default();
    // The first ask moves every row onto an empty set, so it reads cold; the same window asked again arms.
    cache.window(d.path(), &l, 0, 500, "");
    assert!(cache.entries.is_empty() && cache.watches.is_empty());
    cache.window(d.path(), &l, 0, 500, "");
    assert_eq!(cache.entries.len(), ROW_CAP);
    assert!(cache.watches.len() <= WATCH_CAP);
    assert_eq!(cache.inodes.len(), ROW_CAP);
    assert!(cache.bytes <= BYTE_CAP);
    cache.clear();
    super::test_support::assert_cleared(&cache);
    assert!(cache.entries.is_empty());
    assert!(cache.requested.is_empty());
    assert_eq!(cache.inodes.len(), 0);
}

#[test]
#[ignore]
fn search_fb5_probe_rich_rows_latency() {
    let d = TestDir::new("search-row-latency");
    let mut l = Listing::new();
    for i in 0..682 { let name = format!("source_{i:04}.rs"); d.file(&name, "fn main() {}\n"); l.push(&name, false); }
    let body = super::super::mountinfo::read_body();
    let aliases = Aliases::load();
    let thumbs = Thumbnailers::load(&aliases);
    let mime = Db::load();
    let icons = Names::load();
    let mut kinds = Kinds::new();
    for label in ["baseline", "cold", "warm", "intersecting"] {
        let mut cache = Cache::default();
        let mut samples = Vec::new();
        let mut misses = 0;
        let (mut stats, mut xattrs) = (0, 0);
        for iteration in 0..31 {
            if label == "cold" { cache.clear(); }
            let offset = if label == "intersecting" { (iteration % 2) * 100 } else { 0 };
            counts();
            let start = std::time::Instant::now();
            let before = cache.misses;
            let _line = if label == "baseline" {
                let (metas, _) = super::super::meta::stat_range(d.path(), &l, offset, 350);
                rows::rows_line(d.path(), &l, &metas, offset, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds)
            } else {
                let (metas, _) = cache.window(d.path(), &l, offset, 350, &body);
                rows::rows_line_cached(d.path(), &body, &l, &metas, offset, 0.0, &mime, &icons, &aliases, &thumbs, &mut kinds, &mut cache)
            };
            let elapsed = start.elapsed().as_secs_f64() * 1e6;
            let work = counts();
            if iteration > 0 {
                samples.push(elapsed);
                misses += cache.misses - before;
                stats += work.0;
                xattrs += work.1;
            }
        }
        samples.sort_by(f64::total_cmp);
        println!("SEARCH_RICH_ROWS label={label} entries=682 rows=350 p50_us={:.3} p95_us={:.3} misses={misses} stat_calls={stats} tag_reads={xattrs} cache_bytes={}", samples[15], samples[28], cache.bytes);
    }
}

#[test]
fn search_row_cache_overflow_lost_descriptor_and_payload_limit_fail_closed() {
    let d = TestDir::new("search-row-overflow");
    d.file("a.txt", "a");
    let l = listing(&["a.txt"]);
    for mask in [IN_Q_OVERFLOW, IN_IGNORED, IN_UNMOUNT] {
        let mut cache = Cache::default();
        cache.window(d.path(), &l, 0, 1, "");
        assert_eq!(cache.entries.len(), 1);
        let huge = "x".repeat(BYTE_CAP + 1);
        cache.remember(&d.join("a.txt"), &huge, "Data");
        assert!(cache.bytes <= BYTE_CAP);
        assert!(cache.rendered(&d.join("a.txt")).is_none());
        assert!(!cache.burst(&super::super::inotifyburst::event(-1, mask, b"")));
        super::test_support::assert_cleared(&cache);
        assert!(cache.entries.is_empty());
    }
}

#[test]
fn search_row_cache_directory_metadata_changes_when_a_child_is_created() {
    let root = TestDir::new("search-row-directory");
    let folder = root.dir("folder");
    std::fs::File::open(&folder).unwrap().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(12345)).unwrap();
    let mut l = Listing::new();
    l.push("folder", true);
    let mut cache = Cache::default();
    let first = cache.window(root.path(), &l, 0, 1, "").0;
    assert_eq!(first[0].mtime, 12345);
    root.file("folder/new.txt", "a");
    let next = cache.window(root.path(), &l, 0, 1, "").0;
    let fresh = super::super::meta::stat_range(root.path(), &l, 0, 1).0;
    assert_eq!((next[0].size, next[0].mtime), (fresh[0].size, fresh[0].mtime));
    assert_ne!(next[0].mtime, first[0].mtime, "a folder row must watch its own entries as well as its parent");
}

#[test]
fn search_fix2_cache_invalidates_writes_through_an_unrequested_hard_link() {
    let root = TestDir::new("review-fb5-hardlink");
    root.dir("visible");
    root.dir("other");
    let file = root.file("visible/a.txt", "a");
    let alias = root.join("other/alias.txt");
    root.assert_contains(&file);
    root.assert_contains(&alias);
    std::fs::hard_link(&file, &alias).unwrap();
    let l = listing(&["visible/a.txt"]);
    let mut cache = Cache::default();
    let before = render(&mut cache, root.path(), &l, 0, 1, "");
    std::fs::write(&alias, "changed bytes").unwrap();
    let cached = render(&mut cache, root.path(), &l, 0, 1, "");
    let fresh = render(&mut Cache::default(), root.path(), &l, 0, 1, "");
    eprintln!("BEFORE {before}\nCACHED {cached}\nFRESH {fresh}");
    assert_eq!(cached, fresh, "a write through another hard link must invalidate cached facts");
}

#[test]
fn search_fix2_new_hard_links_share_metadata_and_row_invalidation_across_windows() {
    let root = TestDir::new("search-new-hardlink");
    root.dir("visible");
    root.dir("other");
    let file = root.file("visible/a.txt", "a");
    root.file("visible/b.txt", "b");
    let alias = root.join("other/alias.txt");
    let l = listing(&["visible/a.txt", "visible/b.txt"]);
    let mut cache = Cache::default();
    render(&mut cache, root.path(), &l, 0, 1, "");
    std::fs::hard_link(&file, &alias).unwrap();
    for mutation in 0..4 {
        render(&mut cache, root.path(), &l, 0, 1, "");
        match mutation {
            0 => std::fs::write(&alias, "through alias").unwrap(),
            1 => std::fs::set_permissions(&alias, std::fs::Permissions::from_mode(0o700)).unwrap(),
            2 => tags::restore(&alias, &Some(b"Green".to_vec())).unwrap(),
            _ => std::fs::File::options().write(true).open(&alias).unwrap()
                .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(12345)).unwrap(),
        }
        let cached = render(&mut cache, root.path(), &l, 0, 2, "");
        let fresh = render(&mut Cache::default(), root.path(), &l, 0, 2, "");
        assert_eq!(cached, fresh, "alias mutation {mutation} must survive an inode-watch handoff");
    }
}
