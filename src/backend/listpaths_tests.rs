use super::*;
use crate::backend::dirsizeworker::Worker;
use crate::backend::state::{State, Tables};
use crate::backend::testdir::TestDir;
use crate::backend::thumbs::Pool;
use std::sync::{mpsc::channel, Arc};

const FIRST_ROWS: usize = 10;
const NO_WATCH: i32 = -1;

// Supply the worker's precomputed rows to the production landing used by directory listings.
fn worker_listing(path: &str, listing: Listing) -> super::super::iomount::ListOut {
    let base = std::path::Path::new(path);
    let (first_metas, first_ms) = super::super::meta::stat_range(base, &listing, 0, FIRST_ROWS);
    super::super::iomount::ListOut {
        listing,
        read_ms: 0.0,
        sort_ms: 0.0,
        sized: Vec::new(),
        dev: dev_of(base),
        writable: super::super::ops::dir_writable(base),
        first_metas,
        first_start: 0,
        first_ms,
        watch_wd: NO_WATCH,
        watch_mount: std::path::PathBuf::new(),
    }
}

// One State plus its tables and pool, so every answer test starts from the same harness.
fn harness(cache: &std::path::Path) -> (State, Tables, Pool) {
    let (tx, _rx) = crate::backend::events::channel();
    let (st, tb) = (State::new(Worker::new(tx)), Tables::load());
    let (results, _done) = channel();
    let pool = Pool::new(
        1,
        results,
        cache.to_path_buf(),
        Arc::clone(&tb.aliases),
        Arc::clone(&tb.thumbs),
    );
    (st, tb, pool)
}

// One listing of file rows, so old/new pairs read as data.
fn listing_of_names(names: &[&str]) -> Listing {
    let mut l = Listing::new();
    for name in names {
        l.push(name, false);
    }
    l
}

#[test]
fn lists_the_paths_that_exist_in_the_order_they_were_given() {
    let d = TestDir::new("listpaths");
    let file = d.join("b.txt");
    let dir = d.join("a-dir");
    fs::write(&file, "x").unwrap();
    fs::create_dir(&dir).unwrap();
    let asked = vec![
        file.to_string_lossy().to_string(),
        dir.to_string_lossy().to_string(),
    ];
    let (l, _) = listing_of(&asked);
    assert_eq!(l.len(), 2);
    assert_eq!(l.name(0), file.to_string_lossy().strip_prefix('/').unwrap());
    assert!(!l.is_dir(0));
    assert!(l.is_dir(1));
}

#[test]
fn drops_a_path_that_is_gone_rather_than_listing_it() {
    let d = TestDir::new("listpaths-gone");
    let kept = d.join("kept.txt");
    fs::write(&kept, "x").unwrap();
    let asked = vec![
        d.join("never-existed.txt").to_string_lossy().to_string(),
        kept.to_string_lossy().to_string(),
    ];
    let (l, _) = listing_of(&asked);
    assert_eq!(l.len(), 1);
    assert_eq!(l.name(0), kept.to_string_lossy().strip_prefix('/').unwrap());
}

#[test]
fn refuses_a_path_that_is_not_absolute_and_refuses_the_root_itself() {
    let (l, _) = listing_of(&["etc/hostname".to_string(), "".to_string(), "/".to_string()]);
    assert_eq!(l.len(), 0);
}

#[test]
fn a_broken_symlink_is_still_the_entry_the_user_put_here() {
    let d = TestDir::new("listpaths-link");
    let link = d.join("dangling");
    std::os::unix::fs::symlink(d.join("no-such-target"), &link).unwrap();
    let (l, _) = listing_of(&[link.to_string_lossy().to_string()]);
    assert_eq!(l.len(), 1);
    assert!(!l.is_dir(0));
}

#[test]
fn an_empty_request_answers_an_empty_listing() {
    let (l, ms) = listing_of(&[]);
    assert_eq!(l.len(), 0);
    assert!(ms >= 0.0);
}

#[test]
fn a_history_reread_names_one_replaced_path_as_two_changed() {
    // One replaced path is one added plus one removed, so the count is 2.
    let d = TestDir::new("listpaths-changed");
    let a = d.join("a.txt");
    let b = d.join("b.txt");
    let c = d.join("c.txt");
    fs::write(&a, "a").unwrap();
    fs::write(&b, "b").unwrap();
    fs::write(&c, "c").unwrap();
    let astr = a.to_string_lossy().to_string();
    let bstr = b.to_string_lossy().to_string();
    let cstr = c.to_string_lossy().to_string();
    let (mut st, tb, pool) = harness(&d.join("cache"));
    st.base = PathBuf::from("/home/gm");
    let mut out = Vec::new();
    answer(
        &mut out,
        &mut st,
        &pool,
        &tb,
        &[astr.clone(), bstr.clone()],
        10,
        "",
    );
    let first = String::from_utf8(out).unwrap();
    assert!(
        !first.lines().next().unwrap().contains("changed"),
        "a first history listing carries no count: {}",
        first
    );
    let mut out = Vec::new();
    answer(
        &mut out,
        &mut st,
        &pool,
        &tb,
        &[astr.clone(), cstr.clone()],
        10,
        "",
    );
    let second = String::from_utf8(out).unwrap();
    assert!(
        second.lines().next().unwrap().contains("\"changed\":2"),
        "one path replaced reads as two changed: {}",
        second
    );
}

#[test]
fn a_same_path_relist_counts_only_when_the_reload_asked() {
    // One renamed row is one added plus one removed, so the count is 2.
    let d = TestDir::new("listpaths-asked");
    let dir = d.join("dir");
    fs::create_dir(&dir).unwrap();
    let path = dir.to_string_lossy().to_string();
    let (mut st, tb, pool) = harness(&d.join("cache"));
    st.base = PathBuf::from(&path);
    st.listing = listing_of_names(&["a", "b", "c"]);
    let mut out = Vec::new();
    super::super::run::adopt_listed(
        &mut out,
        &mut st,
        &pool,
        &tb,
        &path,
        worker_listing(&path, listing_of_names(&["a", "b", "d"])),
        true,
    );
    let asked = String::from_utf8(out).unwrap();
    assert!(
        asked.lines().next().unwrap().contains("\"changed\":2"),
        "an asked re-list names the rename: {}",
        asked
    );
    st.base = PathBuf::from(&path);
    st.listing = listing_of_names(&["a", "b", "c"]);
    let mut out = Vec::new();
    super::super::run::adopt_listed(
        &mut out,
        &mut st,
        &pool,
        &tb,
        &path,
        worker_listing(&path, listing_of_names(&["a", "b", "d"])),
        false,
    );
    let silent = String::from_utf8(out).unwrap();
    assert!(
        !silent.lines().next().unwrap().contains("changed"),
        "an unasked re-read stays silent: {}",
        silent
    );
}

#[test]
fn a_first_listpaths_after_list_root_carries_no_count() {
    // BASE is "/", so a base-keyed recheck miscounts a root list as a history re-read.
    let d = TestDir::new("listpaths-root");
    let a = d.join("a.txt");
    fs::write(&a, "a").unwrap();
    let astr = a.to_string_lossy().to_string();
    let (mut st, tb, pool) = harness(&d.join("cache"));
    let mut out = Vec::new();
    super::super::run::adopt_listed(
        &mut out,
        &mut st,
        &pool,
        &tb,
        "/",
        worker_listing("/", listing_of_names(&["bin", "etc", "home"])),
        false,
    );
    let mut out = Vec::new();
    answer(&mut out, &mut st, &pool, &tb, &[astr], 10, "");
    let line = String::from_utf8(out).unwrap();
    assert!(
        !line.lines().next().unwrap().contains("changed"),
        "a first listpaths after list / carries no count: {}",
        line
    );
}

#[test]
fn a_list_root_after_listpaths_carries_no_count() {
    // A held history is not a root list, so a root re-list over it names no count.
    let d = TestDir::new("listpaths-held-root");
    let a = d.join("a.txt");
    fs::write(&a, "a").unwrap();
    let astr = a.to_string_lossy().to_string();
    let (mut st, tb, pool) = harness(&d.join("cache"));
    let mut out = Vec::new();
    answer(&mut out, &mut st, &pool, &tb, &[astr], 10, "");
    let mut out = Vec::new();
    super::super::run::adopt_listed(
        &mut out,
        &mut st,
        &pool,
        &tb,
        "/",
        worker_listing("/", listing_of_names(&["bin", "etc", "home"])),
        true,
    );
    assert_eq!(
        st.held,
        Held::List,
        "a directory landing replaces the held history kind"
    );
    let line = String::from_utf8(out).unwrap();
    assert!(
        !line.lines().next().unwrap().contains("changed"),
        "a root list over a held history carries no count: {}",
        line
    );
}
#[test]
fn picker_recent_frames_echo_the_request_generation() {
    let d = TestDir::new("listpaths-picker-generation");
    let file = d.join("a.txt");
    fs::write(&file, "a").unwrap();
    let (mut st, tables, pool) = harness(&d.join("cache"));
    let mut out = Vec::new();
    answer(
        &mut out,
        &mut st,
        &pool,
        &tables,
        &[file.to_string_lossy().into_owned()],
        10,
        r#"{"pickerGen":19}"#,
    );
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 2);
    assert!(text
        .lines()
        .all(|line| crate::json::field_usize(line, "pickerGen") == Some(19)));
}
