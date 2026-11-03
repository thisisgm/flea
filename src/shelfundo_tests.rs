use super::*;
use crate::backend::testdir::TestDir;
use crate::backend::undo::ItemIdentity;
use std::path::PathBuf;

fn moved_step(from: &Path, to: &Path) -> Step {
    Step::Moved {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        before: ItemIdentity::inspect(to).unwrap(),
        after: ItemIdentity::inspect(to).unwrap(),
        rename_fat: false,
    }
}

fn move_of(dir: &Path, name: &str) -> Move {
    let to = dir.join(name);
    let landed = ItemIdentity::inspect(&to).unwrap();
    let (dev, ino, kind) = landed.parts();
    Move { from: dir.join("was").join(name).to_string_lossy().to_string(), to: to.to_string_lossy().to_string(), dev, ino, kind, born: landed.born() }
}

#[test]
fn a_record_comes_back_the_way_it_went_in() {
    let dir = TestDir::new("shelfundo-round");
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    let moves = Moves::at(dir.path());
    let one = move_of(dir.path(), "a.txt");
    moves.record(std::slice::from_ref(&one), 1_700).unwrap();
    assert_eq!(moves.at_ms(), 1_700, "the time is what tells a move from a clear");
    assert_eq!(moves.take().unwrap(), vec![one], "every field of the row survives the file");
    assert!(moves.take().unwrap().is_empty(), "and a record is spent once, so z does not undo twice");
}

#[test]
fn a_move_that_carried_nothing_records_nothing() {
    let dir = TestDir::new("shelfundo-empty");
    let moves = Moves::at(dir.path());
    moves.record(&[], 1_700).unwrap();
    assert_eq!(moves.at_ms(), 0, "nothing happened, so nothing is newer than a clear");
    assert!(moves.take().unwrap().is_empty());
}

#[test]
fn only_the_engines_moved_steps_become_undo_rows() {
    let dir = TestDir::new("shelfundo-steps");
    let landed = dir.path().join("a.txt");
    std::fs::write(&landed, "a").unwrap();
    let steps = vec![
        moved_step(&dir.path().join("was/a.txt"), &landed),
        Step::Created { path: PathBuf::from("/tmp/not-a-move") },
    ];
    let rows = moves_from(&steps);
    assert_eq!(rows.len(), 1, "a copy's own step is not something a shelf move can walk back");
    assert_eq!(rows[0].to, landed.to_string_lossy());
    assert_eq!(rows[0].ino, ItemIdentity::inspect(&landed).unwrap().parts().1, "the identity is the engine's own");
}

// The whole point of recording an identity: the name can be taken again by something else entirely.
#[test]
fn undo_refuses_to_walk_a_stranger_back() {
    let dir = TestDir::new("shelfundo-stranger");
    let landed = dir.path().join("a.txt");
    std::fs::write(&landed, "the file the move carried").unwrap();
    let step = move_of(dir.path(), "a.txt");
    // Keep the inode alive across the unlink, so the replacement cannot reuse it.
    let _held = std::fs::File::open(&landed).unwrap();
    std::fs::remove_file(&landed).unwrap();
    std::fs::write(&landed, "somebody else's file of the same name").unwrap();
    let refused = put_back(&step).expect_err("a different item at that path is not this move's item");
    assert!(refused.contains("was replaced since the move"), "{}", refused);
    assert!(landed.exists(), "and it is left exactly where it is");
}

// Forge only the recorded birth stamp so inode allocation cannot decide this refusal.
#[test]
fn a_row_born_at_another_time_is_a_stranger_and_a_row_with_no_birth_time_is_not() {
    let dir = TestDir::new("shelfundo-born");
    std::fs::create_dir_all(dir.path().join("was")).unwrap();
    let landed = dir.path().join("a.txt");
    std::fs::write(&landed, "a").unwrap();
    let mut step = move_of(dir.path(), "a.txt");
    let Some((sec, nsec)) = step.born else { return };
    step.born = Some((sec.wrapping_add(1), nsec));
    let refused = put_back(&step).expect_err("same device, inode and kind, another birth time");
    assert!(refused.contains("was replaced since the move"), "{}", refused);
    assert!(landed.exists(), "and it is left exactly where it is");
    // A row written before birth time was kept falls back to device, inode and kind.
    step.born = None;
    put_back(&step).expect("nothing contradicts the row");
    assert!(dir.path().join("was/a.txt").exists());
}

#[test]
fn undo_puts_the_file_back_where_the_move_took_it_from() {
    let dir = TestDir::new("shelfundo-back");
    std::fs::create_dir_all(dir.path().join("was")).unwrap();
    let landed = dir.path().join("a.txt");
    std::fs::write(&landed, "a").unwrap();
    let step = move_of(dir.path(), "a.txt");
    put_back(&step).expect("nothing has touched it since");
    assert!(!landed.exists(), "it left the destination");
    assert!(dir.path().join("was/a.txt").exists(), "and it is back where it came from");
}

// A row a hand edit left half-written cannot be reversed safely, so it is dropped rather than guessed.
#[test]
fn a_row_missing_a_field_is_not_a_move() {
    for missing in ["from", "to", "dev", "ino", "kind"] {
        let mut fields = vec![
            ("from".to_string(), Json::Str("/a".to_string())),
            ("to".to_string(), Json::Str("/b".to_string())),
            ("dev".to_string(), Json::Str("66306".to_string())),
            ("ino".to_string(), Json::Str("41".to_string())),
            ("kind".to_string(), Json::Str("32768".to_string())),
        ];
        fields.retain(|(name, _)| name != missing);
        let doc = Json::Obj(vec![("moves".to_string(), Json::Arr(vec![Json::Obj(fields)]))]);
        assert!(moves_of(&doc).is_empty(), "a row with no {} is not reversible", missing);
    }
}

// Sample input: a row with "born":["1789426900","5000"], seconds then nanoseconds, or with no born at all.
#[test]
fn a_row_loads_with_its_birth_time_or_without_one_and_a_malformed_one_is_dropped() {
    let text = |value: &str| Json::Str(value.to_string());
    let doc = |born: Option<Json>| {
        let mut fields = vec![
            ("from".to_string(), text("/a")),
            ("to".to_string(), text("/b")),
            ("dev".to_string(), text("66306")),
            ("ino".to_string(), text("41")),
            ("kind".to_string(), text("32768")),
        ];
        fields.extend(born.map(|b| ("born".to_string(), b)));
        Json::Obj(vec![("moves".to_string(), Json::Arr(vec![Json::Obj(fields)]))])
    };
    assert_eq!(moves_of(&doc(None))[0].born, None, "a row from before birth time was kept still loads");
    let pair = Json::Arr(vec![text("1789426900"), text("5000")]);
    assert_eq!(moves_of(&doc(Some(pair)))[0].born, Some((1_789_426_900, 5000)));
    let too_big = Json::Arr(vec![text("1"), text("4294967296")]);
    for bad in [Json::Null, text("x"), Json::Arr(Vec::new()), Json::Arr(vec![text("1")]), Json::Arr(vec![text("a"), text("1")]), too_big] {
        assert!(moves_of(&doc(Some(bad.clone()))).is_empty(), "a malformed birth time is no move: {:?}", bad);
    }
}

#[test]
fn a_hand_edited_file_that_is_not_the_shelfs_own_undoes_nothing() {
    let dir = TestDir::new("shelfundo-junk");
    let moves = Moves::at(dir.path());
    std::fs::create_dir_all(dir.path().join("omarchy/flea-shelf")).unwrap();
    std::fs::write(dir.path().join("omarchy/flea-shelf/undo.json"), "{ not json").unwrap();
    assert_eq!(moves.at_ms(), 0);
    assert!(moves.take().unwrap_err().contains("left unchanged"));
    assert_eq!(std::fs::read_to_string(&moves.file).unwrap(), "{ not json");
}

// Rule 10 in reverse: the pin the move re-pointed comes home with the file, and the row the move
// took off the pile goes back on it.
#[test]
fn undo_puts_the_pin_back_on_the_path_it_came_from() {
    let dir = TestDir::new("shelfundo-pins");
    std::fs::create_dir_all(dir.path().join("to")).unwrap();
    let home: Vec<String> = ["a.txt", "b.txt"]
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            std::fs::write(&path, "x").unwrap();
            path.to_string_lossy().to_string()
        })
        .collect();
    let landed: Vec<String> = ["a.txt", "b.txt"]
        .iter()
        .map(|name| dir.path().join("to").join(name).to_string_lossy().to_string())
        .collect();
    let shelf = Shelf::at(dir.path());
    shelf.add(&home).unwrap();
    shelf.pin(&[home[1].clone()], true).unwrap();
    // The move itself: the files land, the loose row leaves the pile and the pin follows its file.
    let mut steps = Vec::new();
    for (from, to) in home.iter().zip(&landed) {
        std::fs::rename(from, to).unwrap();
        let landed = ItemIdentity::inspect(Path::new(to)).unwrap();
        let (dev, ino, kind) = landed.parts();
        steps.push(Move { from: from.clone(), to: to.clone(), dev, ino, kind, born: landed.born() });
    }
    shelf.settle(&home).unwrap();
    shelf.repoint(&[(home[1].clone(), landed[1].clone())]).unwrap();
    assert_eq!(shelf.pile(), vec![landed[1].clone()], "the move left one pinned row pointing at the landing");
    // And the walk home, newest first, which is the order settle_back is handed.
    for step in steps.iter().rev() {
        put_back(step).unwrap();
    }
    let back: Vec<&Move> = steps.iter().rev().collect();
    settle_back(&shelf, &back).unwrap();
    assert_eq!(shelf.pile(), vec![home[1].clone(), home[0].clone()],
               "the pin is back on its own path and the loose row is back on the shelf");
    let text = std::fs::read_to_string(shelf.pile_file()).unwrap();
    assert!(text.contains("\"pinned\": true"), "and it is still a pin: {}", text);
}

#[test]
fn a_move_that_left_nothing_at_its_landing_says_why_in_words() {
    let dir = TestDir::new("shelfundogone");
    let to = dir.path().join("never-written").to_string_lossy().to_string();
    let step = Move { from: "/a".to_string(), to: to.clone(), dev: 1, ino: 1, kind: 0, born: None };
    let why = put_back(&step).unwrap_err();
    let reason = crate::error::io_message(&std::io::Error::from(std::io::ErrorKind::NotFound));
    assert_eq!(why, format!("{} is not where the move left it: {}", to, reason));
}

#[test]
fn a_record_with_no_folder_above_it_says_so_in_the_shelfs_words() {
    let dir = TestDir::new("shelfundoroot");
    let mut moves = Moves::at(dir.path());
    moves.file = PathBuf::from("/");
    let step = Move { from: "/a".to_string(), to: "/b".to_string(), dev: 1, ino: 1, kind: 0, born: None };
    assert_eq!(moves.record(&[step], 1_000).unwrap_err(), "the shelf has no folder to write in");
}

#[test]
fn a_record_that_cannot_be_spent_says_why_in_words() {
    let dir = TestDir::new("shelfundospent");
    let moves = Moves::at(dir.path());
    // A folder where the record belongs: removing it is refused, and the refusal is not a missing file.
    std::fs::create_dir_all(&moves.file).unwrap();
    let why = moves.take().unwrap_err();
    assert_eq!(why, format!("{} is not a regular file, so the shelf document was left unchanged", moves.file.display()));
    assert!(!why.contains('('), "no Debug name of an error kind: {}", why);
}
