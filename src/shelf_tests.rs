use super::*;
use crate::backend::testdir::TestDir;

fn shelf(tag: &str) -> (TestDir, Shelf) {
    let dir = TestDir::new(tag);
    let shelf = Shelf::at(dir.path());
    (dir, shelf)
}

fn file(dir: &TestDir, name: &str) -> String {
    dir.file(name, "payload").to_string_lossy().to_string()
}

#[test]
fn only_what_moved_leaves_the_pile() {
    let (dir, shelf) = shelf("shelfsettle");
    let one = file(&dir, "one.txt");
    let two = file(&dir, "two.txt");
    std::fs::create_dir_all(shelf.pile_file().parent().unwrap()).unwrap();
    std::fs::write(
        shelf.pile_file(),
        format!(r#"{{"items":[{{"path":"{}"}},{{"path":"{}"}}]}}"#, one, two),
    )
    .unwrap();
    shelf.settle(std::slice::from_ref(&one)).unwrap();
    assert_eq!(shelf.pile(), vec![two.clone()], "the moved reference leaves and the other stays");
    // Rule 3: a copy reports nothing moved, so the pile is not touched at all.
    shelf.settle(&[]).unwrap();
    assert_eq!(shelf.pile(), vec![two]);
}

#[test]
fn a_pile_that_cannot_be_read_is_an_empty_one_rather_than_an_error() {
    let (dir, shelf) = shelf("shelfbroken");
    std::fs::create_dir_all(shelf.pile_file().parent().unwrap()).unwrap();
    std::fs::write(shelf.pile_file(), "{\"items\":[{\"pa").unwrap();
    assert!(shelf.pile().is_empty());
    let _ = dir;
}

#[test]
fn a_file_answers_its_own_bytes_and_a_directory_its_walk() {
    let (dir, _shelf) = shelf("shelfsize");
    let one = file(&dir, "one.txt");
    let (bytes, partial) = size_of(&one).unwrap();
    assert_eq!(bytes, "payload".len() as u64, "a plain file is its own entry, not a walk");
    assert!(!partial);
    let tree = dir.dir("tree");
    std::fs::write(tree.join("inner.bin"), vec![0u8; 4096]).unwrap();
    let (walked, _) = size_of(&tree.to_string_lossy()).unwrap();
    assert!(walked > 4096, "a directory answers the walk, which counts what is under it: {}", walked);
}

#[test]
fn a_path_that_is_not_there_answers_a_sentence_rather_than_a_zero() {
    let (dir, _shelf) = shelf("shelfsizegone");
    let gone = dir.path().join("never-written").to_string_lossy().to_string();
    assert!(size_of(&gone).is_err(), "a zero would draw as a real size on the card");
}

#[test]
fn the_row_x_takes_the_reference_off_and_leaves_the_file() {
    let (dir, shelf) = shelf("shelfforget");
    let one = file(&dir, "one.txt");
    let two = file(&dir, "two.txt");
    std::fs::create_dir_all(shelf.pile_file().parent().unwrap()).unwrap();
    std::fs::write(
        shelf.pile_file(),
        format!(r#"{{"items":[{{"path":"{}"}},{{"path":"{}"}}]}}"#, one, two),
    )
    .unwrap();
    shelf.settle(std::slice::from_ref(&one)).unwrap();
    assert_eq!(shelf.pile(), vec![two], "the reference leaves the pile");
    assert!(std::fs::metadata(&one).is_ok(), "and the file it named is still there");
}

#[test]
fn a_reference_is_added_once_and_a_folder_says_it_is_one() {
    let (dir, shelf) = shelf("shelfadd");
    let one = file(&dir, "one.txt");
    let tree = dir.dir("tree").to_string_lossy().to_string();
    shelf.add(std::slice::from_ref(&one)).unwrap();
    shelf.add(&[one.clone(), tree.clone()]).unwrap();
    assert_eq!(shelf.pile(), vec![one, tree.clone()], "the second add of a held path changes nothing");
    let text = std::fs::read_to_string(shelf.pile_file()).unwrap();
    assert!(text.contains("\"folder\": true"), "the card draws a folder from the entry, so it is recorded: {}", text);
}

#[test]
fn a_path_that_is_not_there_is_not_added_at_all() {
    let (dir, shelf) = shelf("shelfaddgone");
    let one = file(&dir, "one.txt");
    let gone = dir.path().join("never-written").to_string_lossy().to_string();
    assert!(shelf.add(&[one, gone]).is_err());
    assert!(shelf.pile().is_empty(), "one bad path adds none of them, rather than half a batch");
}

#[test]
fn a_pin_rides_the_pile_entry_and_a_pinned_row_survives_its_own_move() {
    let (dir, shelf) = shelf("shelfpin");
    let one = file(&dir, "one.txt");
    let two = file(&dir, "two.txt");
    shelf.add(&[one.clone(), two.clone()]).unwrap();
    shelf.pin(std::slice::from_ref(&two), true).unwrap();
    let text = std::fs::read_to_string(shelf.pile_file()).unwrap();
    assert!(text.contains("\"pinned\": true"), "the flag rides the entry: {}", text);
    assert_eq!(shelf.pinned_among(&[one.clone(), two.clone()]), vec![two.clone()]);
    // A move takes both: the loose one leaves the pile and the pinned one stays to be re-pointed.
    shelf.settle(&[one.clone(), two.clone()]).unwrap();
    assert_eq!(shelf.pile(), vec![two.clone()], "a pinned row is never consumed");
    shelf.repoint(&[(two.clone(), "/moved/two.txt".to_string())]).unwrap();
    assert_eq!(shelf.pile(), vec!["/moved/two.txt".to_string()], "and the pin follows the file");
    let moved = std::fs::read_to_string(shelf.pile_file()).unwrap();
    assert!(moved.contains("\"pinned\": true"), "the pin is still a pin after the move: {}", moved);
}

#[test]
fn pinning_a_path_the_shelf_is_not_holding_puts_it_on_the_shelf() {
    let (dir, shelf) = shelf("shelfpinnew");
    let one = file(&dir, "one.txt");
    shelf.pin(std::slice::from_ref(&one), true).unwrap();
    assert_eq!(shelf.pile(), vec![one.clone()], "Flea's own menu row pins a path the shelf never held");
    shelf.pin(std::slice::from_ref(&one), false).unwrap();
    assert_eq!(shelf.pile(), vec![one], "unpinning leaves the row on the shelf, loose");
    let text = std::fs::read_to_string(shelf.pile_file()).unwrap();
    assert!(text.contains("\"pinned\": false"), "{}", text);
}

// SettingsRest rule 4: the panel orders the pins, and the loose entries keep their own places.
#[test]
fn order_moves_a_pin_among_the_pins_only() {
    let (dir, shelf) = shelf("shelforder");
    shelf.add(&[file(&dir, "loose.txt")]).unwrap();
    shelf.pin(&[file(&dir, "one.txt"), file(&dir, "two.txt")], true).unwrap();
    let two_again = file(&dir, "two.txt");
    shelf.order(&two_again, 0).unwrap();
    let leaves: Vec<String> = shelf
        .pile()
        .iter()
        .map(|path| path.rsplit('/').next().unwrap_or(path).to_string())
        .collect();
    assert_eq!(leaves, vec!["loose.txt", "two.txt", "one.txt"]);
}

#[test]
fn order_ignores_a_path_that_is_not_pinned_and_a_place_that_is_not_there() {
    let (dir, shelf) = shelf("shelforderbad");
    let one = file(&dir, "one.txt");
    shelf.pin(std::slice::from_ref(&one), true).unwrap();
    shelf.order("/nowhere", 0).unwrap();
    shelf.order(&one, 9).unwrap();
    assert_eq!(shelf.pile().len(), 1);
}

// Found in code review: a pile that cannot be parsed is not an empty one, and writing over it
// would take every row with it.
#[test]
fn a_pile_that_cannot_be_parsed_is_never_written_over() {
    let (dir, shelf) = shelf("shelfbadjson");
    let one = file(&dir, "one.txt");
    shelf.add(std::slice::from_ref(&one)).unwrap();
    std::fs::write(shelf.pile_file(), b"{\"items\": [ truncated").unwrap();
    let refused = shelf.add(&[file(&dir, "two.txt")]);
    assert!(refused.is_err(), "a pile this cannot read is not an empty pile");
    let kept = std::fs::read_to_string(shelf.pile_file()).unwrap();
    assert!(kept.contains("truncated"), "the operator's own file stayed exactly as it was");
}

// A pinned row whose file was deleted outside Flea is exactly the row that has to be unpinnable.
#[test]
fn a_pin_can_be_taken_off_a_file_that_is_gone() {
    let (dir, shelf) = shelf("shelfghostpin");
    let one = file(&dir, "one.txt");
    shelf.pin(std::slice::from_ref(&one), true).unwrap();
    std::fs::remove_file(&one).unwrap();
    shelf.pin(std::slice::from_ref(&one), false).unwrap();
    let pinned = shelf.pinned_among(&[one]);
    assert!(pinned.is_empty(), "the pin came off, {:?}", pinned);
}

// absolute() is the 1.77 spelling of std::path::absolute: join cwd, drop CurDir, keep // and one trailing slash, refuse empty.
#[test]
fn absolute_keeps_the_std_contract_it_replaces() {
    use std::os::unix::ffi::OsStrExt;
    // Raw bytes, because PathBuf equality compares components and forgives a lost // or slash.
    let read = |input: &str| super::absolute(input).expect("an absolute form").as_os_str().as_bytes().to_vec();
    assert_eq!(read("/tmp/x/./one.txt"), b"/tmp/x/one.txt");
    assert_eq!(read("/tmp/x/../x/one.txt"), b"/tmp/x/../x/one.txt");
    assert_eq!(read("/tmp/x/."), b"/tmp/x");
    assert_eq!(read("/tmp/x/"), b"/tmp/x/");
    assert_eq!(read("//tmp/x"), b"//tmp/x");
    assert_eq!(read("//"), b"//");
    assert_eq!(read("///tmp/x"), b"/tmp/x");
    assert!(super::absolute("").is_err(), "an empty path has no absolute form");
    // Joining off / keeps its sole slash as the separator, so the want is /rel/path and never //rel/path.
    let want = std::env::current_dir().expect("a working directory").join("rel/path").as_os_str().as_bytes().to_vec();
    assert_eq!(read("rel/path"), want);
}

// A reason comes from error::io_message, so a person reads words and not Os { code: 21, kind: IsADirectory }.
fn reason(kind: std::io::ErrorKind) -> String {
    crate::error::io_message(&std::io::Error::from(kind))
}

#[test]
fn a_pile_that_cannot_be_read_says_why_in_words() {
    let (_dir, shelf) = shelf("shelfreadwhy");
    // A folder where the pile file belongs: the read is refused with a reason a person can act on.
    std::fs::create_dir_all(shelf.pile_file()).unwrap();
    assert_eq!(shelf.held().unwrap_err(), format!("{} is not a regular file, so the shelf document was left unchanged", shelf.pile_file().display()));
}

#[test]
fn an_item_that_is_not_there_is_refused_with_its_reason() {
    let (dir, shelf) = shelf("shelfaddgone");
    let gone = dir.path().join("never-written").to_string_lossy().to_string();
    let expected = format!("{} could not be read: {}", gone, reason(std::io::ErrorKind::NotFound));
    assert_eq!(shelf.add(std::slice::from_ref(&gone)).unwrap_err(), expected);
    assert_eq!(size_of(&gone).unwrap_err(), expected);
}

#[test]
fn a_pile_file_with_no_folder_above_it_says_so_in_the_shelfs_words() {
    let (_dir, mut shelf) = shelf("shelfnofolder");
    // The root has no parent, so there is no folder to write the pile in.
    shelf.pile = PathBuf::from("/");
    assert_eq!(shelf.clear().unwrap_err(), "the shelf has no folder to write in");
}

#[test]
fn an_empty_or_blank_pile_file_is_an_empty_pile_as_0312_read_it() {
    for (tag, bytes) in [("shelfemptypile", ""), ("shelfblankpile", " \n\t\n")] {
        let (dir, shelf) = shelf(tag);
        std::fs::create_dir_all(shelf.pile_file().parent().unwrap()).unwrap();
        std::fs::write(shelf.pile_file(), bytes).unwrap();
        assert_eq!(shelf.held(), Ok(Vec::new()), "{tag}: the read is an empty pile");
        let one = file(&dir, "one.txt");
        assert!(shelf.add(std::slice::from_ref(&one)).is_ok(), "{tag}: the first add lands");
        assert_eq!(shelf.pile(), vec![one], "{tag}: and the pile holds it");
    }
}
