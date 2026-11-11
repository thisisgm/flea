//! tests/js/trashdrop.js through stand-ins for the drag owner's marker parser, uri reader and the GVFS test.
use super::dates_tests::english;
use super::dropping::{line, marker_rows, request, trashed_first, Marker, Request};

const INSTANCE: &str = "1760000000-42";

// Drag.markerPayload: sender, rows, ctrl, source, device, deletable, shift, listing.
fn payload(rows: &[&str], source: &str, listing: u64) -> String {
    format!("{INSTANCE}\n{}\nmove\n{}\n42\n1\n0\n{listing}", rows.join(","), source.replace('/', "%2F"))
}

// Stand-in for Drag.markerFields and isOwnDrag; `lifted` is the marker this window's backend recorded.
fn marker<'a>(raw: &'a str, source: &'a str, lifted: &str) -> Option<Marker<'a>> {
    let fields: Vec<&str> = raw.split('\n').collect();
    if fields.len() != 8 || fields[0] != INSTANCE || !fields[7].bytes().all(|b| b.is_ascii_digit()) || fields[7].is_empty() {
        return None;
    }
    Some(Marker { raw, source, listing: fields[7].parse().ok()?, lifted_here: raw == lifted })
}

// Stand-in for Mounts.trashable: nothing under the GVFS FUSE root has a trash.
fn trashable(path: &str) -> bool {
    !(path.starts_with("/run/user/") && path.contains("/gvfs/") && path.split("/gvfs/").nth(1).is_some_and(|rest| rest.contains(':')))
}

// Stand-in for DragOut.filePaths: file uris only.
fn paths(urls: &[&str]) -> Vec<String> {
    urls.iter().filter_map(|url| url.strip_prefix("file://")).filter(|p| p.starts_with('/')).map(String::from).collect()
}

#[test]
fn trashdrop_own_paths_trash_and_the_cursor_lands() {
    let i18n = english();
    let raw = payload(&["1", "3"], "/d", 7);
    let urls = ["file:///d/a.txt", "file:///d/c.txt"];
    let own = marker(&raw, "/d", &raw);
    let sent = request(own, &paths(&urls), "/d", trashable);
    assert_eq!(sent, Some(Request::Paths(vec!["/d/a.txt".into(), "/d/c.txt".into()])), "the drop trashes those paths");
    assert_eq!(sent.as_ref().unwrap().line(), r#"{"c":"trash","paths":["/d/a.txt","/d/c.txt"]}"#, "one trash of the carried paths");
    assert_eq!(trashed_first(own, "/d"), Some(1), "the cursor lands where the first trashed row was");
    assert_eq!(line(&i18n, sent.as_ref(), &raw), "Move 2 items to Trash", "the bar says what the drop will do");
    let elsewhere = request(own, &paths(&urls), "/e", trashable);
    assert_eq!(elsewhere, sent, "paths still trash from another listing, naming the same files");
    assert_eq!(trashed_first(own, "/e"), None, "with no cursor landing in a listing they were not in");
}

#[test]
fn trashdrop_refuses_what_this_window_did_not_lift() {
    let i18n = english();
    let raw = payload(&["1"], "/d", 7);
    assert_eq!(request(marker("", "/d", &raw), &paths(&["file:///d/a.txt"]), "/d", trashable), None, "a foreign drag is refused");
    let other = format!("other-flea{}", &raw[raw.find('\n').unwrap()..]);
    assert_eq!(request(marker(&other, "/d", &raw), &paths(&["file:///d/a.txt"]), "/d", trashable), None, "another Flea's drag is refused");
    let own = marker(&raw, "/d", "");
    assert_eq!(request(own, &paths(&["https://example.com/a.txt"]), "/d", trashable), None,
        "a non-file uri is no path, and an unrecorded marker names no rows");
    let gvfs = "/run/user/1000/gvfs/smb-share:server=nas,share=data";
    let mounted = payload(&["1"], gvfs, 7);
    assert_eq!(request(marker(&mounted, gvfs, &mounted), &paths(&[&format!("file://{gvfs}/a.txt")]), gvfs, trashable), None,
        "a drag lifted on a GVFS mount is refused, as d is");
    assert_eq!(line(&i18n, None, &raw), "", "a refused drag says nothing");
}

#[test]
fn trashdrop_wide_rows_name_their_lift_numbering_and_backend() {
    let i18n = english();
    let wide = payload(&["3", "0", "2"], "/d", 7);
    let other_backend = marker(&wide, "/d", "");
    assert_eq!(request(other_backend, &[], "/d", trashable), None, "equal generations in another backend never authorize the rows");
    assert_eq!(line(&i18n, None, &wide), "", "another backend says nothing for the refused rows");
    let unrecorded = payload(&["1"], "/d", 7);
    assert_eq!(request(marker(&unrecorded, "/d", &wide), &[], "/d", trashable), None, "an unrecorded marker cannot borrow the lifting backend");
    let here = marker(&wide, "/d", &wide);
    assert_eq!(request(here, &[], "/e", trashable), None, "the lifting backend still refuses a different directory");
    let carried = request(other_backend, &paths(&["file:///d/c.txt"]), "/d", trashable);
    assert_eq!(carried, Some(Request::Paths(vec!["/d/c.txt".into()])), "paths work through another backend on the same folder");
    assert_eq!(trashed_first(other_backend, "/d"), None, "paths through another backend have no cursor rows");
    let rows = request(here, &[], "/d", trashable);
    assert_eq!(rows.as_ref().map(Request::line), Some(r#"{"c":"trash","rows":[3,0,2],"listing":7}"#.into()),
        "rows named in the numbering of the lift, not the one held now");
    assert_eq!(line(&i18n, rows.as_ref(), &wide), "Move 3 items to Trash", "the bar counts the rows the marker carries");
    assert_eq!(trashed_first(here, "/d"), Some(0), "the cursor lands on the minimum inside an unsorted selection");
}

#[test]
fn trashdrop_uncapped_and_malformed_rows() {
    let all: Vec<String> = (1..=1_000_000).rev().map(|i: usize| (i + 16).to_string()).collect();
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();
    let raw = payload(&refs, "/d", 7);
    let here = marker(&raw, "/d", &raw);
    match request(here, &[], "/d", trashable) {
        Some(Request::Rows { rows, listing }) => {
            assert_eq!(listing, 7);
            assert!(rows.len() == all.len() && rows.iter().zip(&all).all(|(row, text)| row.to_string() == *text),
                "every selected row keeps its lift-time numbering");
        }
        other => panic!("an uncapped selection reaches the backend, got {:?}", other.map(|r| r.line().len())),
    }
    assert_eq!(trashed_first(here, "/d"), Some(17), "the cursor lands on the minimum even when it is last");
    let none = payload(&["1"], "/d", 0);
    assert_eq!(request(marker(&none, "/d", &none), &[], "/d", trashable), None, "a wide drag whose lift had no numbering is refused");
    let mixed = payload(&["1", "x"], "/d", 7);
    assert_eq!(request(marker(&mixed, "/d", &mixed), &[], "/d", trashable), None, "a malformed row field is refused whole");
    for bad in ["1,,3", "1,", ",1", "-1", " 1", "+1", "1.0", "1e0"] {
        let raw = payload(&[bad], "/d", 7);
        assert!(marker_rows(&raw).is_empty(), "row field {bad} parses as no rows");
        assert_eq!(request(marker(&raw, "/d", &raw), &[], "/d", trashable), None, "row field {bad} refuses the whole drop");
    }
}
