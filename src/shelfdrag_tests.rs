use super::*;
use crate::backend::testdir::TestDir;
use crate::shelf::Shelf;

fn shelf(tag: &str) -> (TestDir, Shelf) {
    let dir = TestDir::new(tag);
    let shelf = Shelf::at(dir.path());
    (dir, shelf)
}

fn file(dir: &TestDir, name: &str) -> String {
    dir.file(name, "payload").to_string_lossy().to_string()
}

#[test]
fn a_token_names_the_entries_and_the_intent_the_lift_fixed() {
    let (dir, shelf) = shelf("shelfdrag");
    let one = file(&dir, "one.txt");
    let token = shelf.drag_begin(true, std::slice::from_ref(&one), 1_000).unwrap();
    assert_eq!(token.len(), TOKEN_BYTES * 2, "sixteen bytes of randomness, hex encoded");
    let redeemed = shelf.redeem(&token, 1_500).unwrap();
    assert!(redeemed.moving, "a plain shelf drag asks for a move");
    assert_eq!(redeemed.paths, vec![one]);
}

#[test]
fn a_token_is_spent_the_first_time_it_is_redeemed() {
    let (dir, shelf) = shelf("shelfonce");
    let token = shelf.drag_begin(false, &[file(&dir, "one.txt")], 1_000).unwrap();
    let first = shelf.redeem(&token, 1_100).expect("the first redemption");
    assert!(!first.moving, "copy is the other half of the intent the lift fixed");
    let again = shelf.redeem(&token, 1_200);
    assert!(again.is_err(), "a replayed token finds nothing: {:?}", again.map(|r| r.paths));
}

#[test]
fn a_token_older_than_a_gesture_is_refused() {
    let (dir, shelf) = shelf("shelfstale");
    let token = shelf.drag_begin(true, &[file(&dir, "one.txt")], 1_000).unwrap();
    assert!(shelf.redeem(&token, 1_000 + TOKEN_LIFE_MS).is_err(), "a drag does not outlive its own gesture");
}

#[test]
fn a_token_nobody_minted_is_refused() {
    let (_dir, shelf) = shelf("shelfforged");
    assert!(shelf.redeem("deadbeefdeadbeefdeadbeefdeadbeef", 1_000).is_err());
}

#[test]
fn an_entry_that_is_not_the_file_it_was_is_refused() {
    let (dir, shelf) = shelf("shelfswapped");
    let one = file(&dir, "one.txt");
    let token = shelf.drag_begin(true, std::slice::from_ref(&one), 1_000).unwrap();
    // The same name, another inode: renamed over rather than deleted and rewritten, because a freed inode is commonly handed straight back to the next file created in the same group.
    let other = file(&dir, "other.txt");
    std::fs::write(&other, "another file entirely").unwrap();
    std::fs::rename(&other, &one).unwrap();
    let refused = shelf.redeem(&token, 1_100);
    assert!(refused.is_err(), "a path that now names another inode is not what was lifted");
}

#[test]
fn a_drag_of_nothing_is_refused_before_a_token_exists() {
    let (_dir, shelf) = shelf("shelfempty");
    assert!(shelf.drag_begin(true, &[], 1_000).is_err());
}

#[test]
fn a_drag_records_the_absolute_path_the_pile_holds() {
    let (dir, shelf) = shelf("shelfrelative");
    let one = file(&dir, "one.txt");
    // Spelled with a redundant component, which is what absolute() takes back out: a token that kept the spelling it was handed would answer with a path the pile never held.
    let spelled = format!("{}/./{}", dir.path().display(), "one.txt");
    let token = shelf.drag_begin(true, &[spelled], 1_000).unwrap();
    assert_eq!(shelf.redeem(&token, 1_100).unwrap().paths, vec![one]);
}

fn reason(kind: std::io::ErrorKind) -> String {
    crate::error::io_message(&std::io::Error::from(kind))
}

#[test]
fn a_lift_of_something_that_is_gone_says_why_in_words() {
    let (dir, shelf) = shelf("shelfdraggone");
    let gone = dir.path().join("never-written").to_string_lossy().to_string();
    let why = shelf.drag_begin(true, std::slice::from_ref(&gone), 1_000).unwrap_err();
    assert_eq!(why, format!("{} could not be read: {}", gone, reason(std::io::ErrorKind::NotFound)));
}

#[test]
fn a_drag_file_with_no_folder_above_it_says_so_in_the_shelfs_words() {
    let (dir, mut shelf) = shelf("shelfdragroot");
    shelf.drags = std::path::PathBuf::from("/");
    let why = shelf.drag_begin(true, &[file(&dir, "one.txt")], 1_000).unwrap_err();
    assert_eq!(why, "the shelf has no folder to write in");
}
