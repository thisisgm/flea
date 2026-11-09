//! One outline request opens or closes every named folder and keeps the anchor row.
use super::*;

#[test]
fn tree_outline_one_request_opens_every_named_folder_and_keeps_the_anchor_row() {
    use crate::backend::{dirsizeworker::Worker, outline, proto::{parse_request, Request}, state::State};
    let dir = TestDir::new("tree-outline-many");
    dir.dir("A");
    dir.dir("B");
    dir.file("c.txt", "c");
    let (tx, _) = events::channel();
    let mut state = State::new(Worker::new(tx));
    state.base = dir.path().to_owned();
    state.listing = listed(&dir, &[], "").listing;
    let base = dir.path().to_string_lossy().into_owned();
    state.outline.last = format!(r#"{{"c":"list","path":"{}"}}"#, crate::json::escape(&base));
    let line = r#"{"c":"outline","rows":[0,1],"open":true,"anchor":2,"listing":1,"first":32}"#;
    let Request::Outline { rows, anchor, open, .. } = parse_request(line) else { panic!("not an outline request") };
    assert_eq!((rows.as_slice(), anchor, open), (&[0, 1][..], Some(2), true));
    let opened = outline::request(&mut state, &rows, anchor, open, None, 32).unwrap();
    assert!(state.outline.open.contains(&dir.join("A")) && state.outline.open.contains(&dir.join("B")));
    assert_eq!(crate::json::field_str(&opened, "select"), Some(dir.join("c.txt").to_string_lossy().into_owned()));
    outline::request(&mut state, &[0, 1], None, false, None, 32).unwrap();
    assert!(state.outline.open.is_empty(), "one close request closes every named folder");
    assert_eq!(
        outline::request(&mut state, &[2, 9], None, true, None, 32),
        Err("row 2 is not a folder in this listing".into())
    );
    assert!(state.outline.open.is_empty(), "a request naming no folder opens nothing");
    outline::request(&mut state, &[2, 0, 9], None, true, None, 32).unwrap();
    assert_eq!(state.outline.open.len(), 1, "a marked file or a row past the listing is skipped");
    assert!(state.outline.open.contains(&dir.join("A")));
    outline::request(&mut state, &[2, 9], Some(1), true, None, 32).unwrap();
    assert!(state.outline.open.contains(&dir.join("B")), "with no folder named, the anchor's folder opens");
    state.outline.open.remove(&dir.join("B"));
    // With A open, closing it from a cursor on A's child selects A, the row the child folds into.
    dir.file("A/in.txt", "i");
    state.listing = listed(&dir, &["A"], "").listing;
    let child = (0..state.listing.len()).find(|&row| state.listing.name(row) == "A/in.txt").expect("A/in.txt is listed under A");
    let closed = outline::request(&mut state, &[0], Some(child), false, None, 32).unwrap();
    assert_eq!(crate::json::field_str(&closed, "select"), Some(dir.join("A").to_string_lossy().into_owned()));
    let kept = outline::request(&mut state, &[0], Some(child), true, None, 32).unwrap();
    assert_eq!(crate::json::field_str(&kept, "select"), Some(dir.join("A/in.txt").to_string_lossy().into_owned()));
    let Request::Outline { rows, .. } = parse_request(r#"{"c":"outline","row":1,"open":true}"#) else { panic!("not an outline request") };
    assert_eq!(rows, [1], "a lone row still names one folder");
    std::os::unix::fs::symlink(dir.join("B"), dir.join("L")).unwrap();
    state.listing = listed(&dir, &[], "").listing;
    let link = (0..state.listing.len()).find(|&row| state.listing.name(row) == "L").expect("L is listed");
    assert_eq!(outline::request(&mut state, &[link], None, true, None, 32), Err(format!("row {link} is not a folder in this listing")));
    assert!(!state.outline.open.contains(&dir.join("L")), "a symlinked folder never opens in place");
}
