use super::*;
use crate::backend::searchsession::Session;
use crate::backend::testdir::TestDir;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

// "caf" then 0xe9, Latin-1 e-acute: not UTF-8. Its lossy text is "caf" then U+FFFD.
const RAW_FILE: &[u8] = b"caf\xe9.txt";
const RAW_DIR: &[u8] = b"d\xe9";
const LOSSY_FILE: &str = "caf\u{fffd}.txt";

fn raw(d: &TestDir, name: &[u8]) -> PathBuf {
    d.path().join(OsStr::from_bytes(name))
}

// caf?.txt carries Red, notes.txt carries nothing, d?/needle.txt sits under a directory that is not UTF-8.
fn fixture(tag: &str) -> TestDir {
    let d = TestDir::new(tag);
    std::fs::write(raw(&d, RAW_FILE), "").unwrap();
    d.file("notes.txt", "");
    std::fs::create_dir(raw(&d, RAW_DIR)).unwrap();
    std::fs::write(raw(&d, RAW_DIR).join("needle.txt"), "").unwrap();
    crate::backend::tags::write_names(&raw(&d, RAW_FILE), &["Red".to_string()]).unwrap();
    d
}

fn open(d: &TestDir) -> Session {
    let scope = d.path().to_str().expect("utf-8").to_string();
    Session::open_sync(&scope, &scope, false)
}

fn hits(s: &mut Session, gen: usize, query: &str) -> Vec<String> {
    let answer = s.query(gen, query, 100).expect("a newer generation answers");
    answer.rows.iter().map(|(path, _)| path.clone()).collect()
}

// A tag query on a session with no event receiver: the first ask starts the read, the second finds the names in.
fn tag_hits(s: &mut Session, gen: usize, query: &str) -> Vec<String> {
    hits(s, gen, query);
    s.tags.wait_read();
    hits(s, gen + 1, query)
}

// Every row's path on disk, the way a consumer joins it to the scope.
fn on_disk(scope: &Path, index: &Index) -> Vec<(String, bool)> {
    (0..index.len()).map(|row| (index.path(row).to_string(), scope.join(OsStr::from_bytes(&index.raw(row))).exists())).collect()
}

#[test]
fn every_row_of_the_index_names_a_file_the_walk_found() {
    let d = fixture("rawrows");
    let s = open(&d);
    let rows = on_disk(d.path(), &s.index.lock().unwrap());
    let mut names: Vec<&str> = rows.iter().map(|(name, _)| name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["caf\u{fffd}.txt", "d\u{fffd}", "d\u{fffd}/needle.txt", "notes.txt"], "4 rows over 2 directories walked");
    assert!(rows.iter().all(|(_, exists)| *exists), "a row that names no file on disk: {:?}", rows);
}

#[test]
fn a_tag_term_finds_a_tagged_file_whose_name_is_not_utf8() {
    let d = fixture("rawtag");
    let mut s = open(&d);
    assert_eq!(tag_hits(&mut s, 1, "tag:Red"), [LOSSY_FILE], "the tag is read at the file's own path");
}

#[test]
fn a_directory_whose_name_is_not_utf8_is_walked_in() {
    let d = fixture("rawdir");
    let mut s = open(&d);
    assert_eq!(hits(&mut s, 1, "needle"), ["d\u{fffd}/needle.txt"]);
}

#[test]
fn a_tag_term_reaches_a_file_under_a_directory_that_is_not_utf8() {
    let d = fixture("rawdirtag");
    crate::backend::tags::write_names(&raw(&d, RAW_DIR).join("needle.txt"), &["Blue".to_string()]).unwrap();
    let mut s = open(&d);
    assert_eq!(tag_hits(&mut s, 1, "tag:Blue"), ["d\u{fffd}/needle.txt"]);
}

#[test]
fn a_patch_after_a_write_indexes_a_new_name_that_is_not_utf8() {
    let d = fixture("rawwrite");
    let mut s = open(&d);
    let added = raw(&d, b"new\xe9.jpg");
    std::fs::write(&added, "").unwrap();
    crate::backend::tags::write_names(&added, &["Green".to_string()]).unwrap();
    assert!(s.patch_after_write(&added), "the own write under the scope patches the index");
    assert_eq!(tag_hits(&mut s, 1, "tag:Green"), ["new\u{fffd}.jpg"]);
}

#[test]
fn a_patch_replaces_a_row_renamed_to_a_name_with_the_same_lossy_text() {
    let d = fixture("rawrename");
    let mut s = open(&d);
    std::fs::rename(raw(&d, RAW_FILE), raw(&d, b"caf\xe8.txt")).unwrap();
    assert!(s.patch_open_dir(d.path()), "the scope itself is under the scope");
    let rows = on_disk(d.path(), &s.index.lock().unwrap());
    assert!(rows.iter().all(|(_, exists)| *exists), "a stale row names no file on disk: {:?}", rows);
    assert_eq!(hits(&mut s, 1, "caf"), [LOSSY_FILE], "one row for the one file left");
}

#[test]
fn a_patch_drops_a_row_whose_name_is_not_utf8_once_the_file_is_gone() {
    let d = fixture("rawgone");
    let mut s = open(&d);
    std::fs::remove_file(raw(&d, RAW_FILE)).unwrap();
    assert!(s.patch_open_dir(d.path()), "the scope itself is under the scope");
    assert!(hits(&mut s, 1, "caf").is_empty(), "the removed file leaves no row");
}

// The listing, the walk route and the session all name the file the same way: the wire has no byte escape.
#[test]
fn the_session_names_the_file_as_the_listing_and_the_walk_route_do() {
    let d = fixture("rawwire");
    let root = d.path().to_str().expect("utf-8");
    let (listing, _) = crate::backend::scan::scan(root, false).expect("scan");
    let listed: Vec<String> = (0..listing.len()).map(|i| listing.name(i).to_string()).collect();
    assert!(listed.contains(&LOSSY_FILE.to_string()), "the listing row is lossy: {:?}", listed);
    let mut search = crate::backend::search::Search::new(root, "caf", false);
    let mut walked = crate::backend::listing::Listing::new();
    while !search.step(&mut walked) {}
    assert_eq!(walked.name(0), LOSSY_FILE, "the walk route hit is lossy");
    let mut s = open(&d);
    assert_eq!(hits(&mut s, 1, "caf"), [LOSSY_FILE], "the session hit is lossy the same way");
}

// Diagnostic only: the bytes a 40,000-name UTF-8 tree costs, so the raw bytes can be shown to add none.
#[test]
fn a_utf8_tree_costs_the_bytes_it_always_did() {
    let mut index = Index::new();
    for i in 0..40_000 {
        index.push(format!("some_directory_name_{:03}/nested_subdir/report_final_draft_v{:05}.txt", i % 200, i), false);
    }
    assert_eq!(index.names.len(), 40_201, "each distinct raw basename is interned once");
    eprintln!("INDEX_BYTES {} over {} rows", index_bytes(&index), index.len());
}
