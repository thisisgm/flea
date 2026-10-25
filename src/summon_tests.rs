use super::*;
use crate::backend::testdir::TestDir;

fn pile_of(paths: &[&str]) -> Vec<Json> {
    paths
        .iter()
        .map(|path| Json::Obj(vec![("path".to_string(), Json::Str((*path).to_string()))]))
        .collect()
}

#[test]
fn the_bind_rings_a_bell_rather_than_setting_a_state() {
    let dir = TestDir::new("summon-ring");
    let summon = Summon::at(dir.path());
    assert_eq!(summon.rings(), 0, "a shelf nobody has summoned has rung nothing");
    assert_eq!(summon.ring().unwrap(), 1);
    assert_eq!(summon.ring().unwrap(), 2, "each press is its own ring, so the card and the file cannot disagree");
    assert_eq!(summon.rings(), 2);
}

#[test]
fn only_the_last_five_piles_are_kept_and_the_newest_is_first() {
    let dir = TestDir::new("summon-keep");
    let summon = Summon::at(dir.path());
    for i in 0..7 {
        summon.keep(pile_of(&[&format!("/p/{}", i)]), 1_000 + i as u64).unwrap();
    }
    let piles = summon.piles();
    assert_eq!(piles.len(), KEPT_PILES);
    assert_eq!(piles[0].get("at").and_then(Json::as_f64), Some(1_006.0), "the newest clear is the first row");
    assert_eq!(piles[4].get("at").and_then(Json::as_f64), Some(1_002.0), "and the two oldest have fallen off");
}

#[test]
fn an_empty_pile_is_not_worth_keeping() {
    let dir = TestDir::new("summon-empty");
    let summon = Summon::at(dir.path());
    summon.keep(Vec::new(), 1_000).unwrap();
    assert!(summon.piles().is_empty(), "clearing an empty shelf must not push a row nobody can use");
}

#[test]
fn a_restored_pile_leaves_the_history_with_it() {
    let dir = TestDir::new("summon-take");
    let summon = Summon::at(dir.path());
    summon.keep(pile_of(&["/p/old"]), 1_000).unwrap();
    summon.keep(pile_of(&["/p/new"]), 2_000).unwrap();
    let taken = summon.restore_pile(0).unwrap();
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].get("path").and_then(Json::as_str), Some("/p/new"));
    assert_eq!(summon.piles().len(), 1, "the same entry cannot be restored twice");
    assert!(summon.restore_pile(4).is_err(), "a row the card never drew is refused");
}

#[test]
fn a_history_file_that_cannot_be_read_reads_as_no_history_and_is_never_written_over() {
    let dir = TestDir::new("summon-broken");
    let summon = Summon::at(dir.path());
    let dir_of = summon.piles_file().parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&dir_of).unwrap();
    std::fs::write(summon.piles_file(), "{\"piles\":[{\"at").unwrap();
    assert!(summon.piles().is_empty());
    // A summon file that is there and unreadable, so the count answers zero from the parse rather
    // than from a file that was never written.
    std::fs::write(dir_of.join("summon.json"), "{\"summon\":").unwrap();
    assert_eq!(summon.rings(), 0);
    assert!(summon.ring().unwrap_err().contains("left unchanged"));
    assert_eq!(std::fs::read_to_string(dir_of.join("summon.json")).unwrap(), "{\"summon\":");
}

#[test]
fn the_chord_is_read_off_the_line_in_the_users_own_config() {
    let config = "o.bind(\"SUPER + RETURN\", \"Terminal\", { omarchy = \"terminal\" })\n\
                  o.bind(\"SUPER + SHIFT + D\", \"Drop shelf\", \"flea shelf toggle\")\n";
    assert_eq!(summon_chord(config), Some("super+shift+d".to_string()));
    // The README offers the line, so a user who has only read it has not installed it.
    assert_eq!(summon_chord("-- o.bind(\"SUPER + D\", \"Drop shelf\", \"flea shelf toggle\")\n"), None,
               "a commented line is a suggestion, not a bind");
    assert_eq!(summon_chord("o.bind(\"SUPER + SHIFT + F\", \"File manager\", \"flea --gui\")\n"), None,
               "a bind that opens Flea itself is not the shelf's own");
}

// The card numbers its menu rows from one and sends that number, so this is the conversion it
// depends on: row one is the newest pile, and anything that is not a row number is refused.
#[test]
fn the_pile_number_the_card_sends_is_one_based() {
    assert_eq!(chosen_index(&[]), Ok(0), "no argument at all is the newest pile");
    assert_eq!(chosen_index(&["1".to_string()]), Ok(0));
    assert_eq!(chosen_index(&["5".to_string()]), Ok(4));
    assert!(chosen_index(&["0".to_string()]).is_err(), "there is no row zero on the card");
    assert!(chosen_index(&["two".to_string()]).is_err(), "and a word is not a row number");
}

#[test]
fn files_with_no_folder_above_them_say_so_in_the_shelfs_words() {
    let dir = TestDir::new("summon-root");
    let mut summon = Summon::at(dir.path());
    summon.summon = std::path::PathBuf::from("/");
    assert_eq!(summon.ring().unwrap_err(), "the shelf has no folder to write in");
    let mut summon = Summon::at(dir.path());
    summon.piles = std::path::PathBuf::from("/");
    assert_eq!(summon.keep(pile_of(&["/p/a"]), 1_000).unwrap_err(), "the shelf has no folder to write in");
    assert_eq!(summon.restore_pile(0).unwrap_err(), "the shelf has no folder to write in");
}

#[test]
fn a_pile_that_neither_the_shelf_nor_the_history_would_take_says_what_happened() {
    assert_eq!(
        pile_lost("busy"),
        "the pile could not be restored, and it could not go back into the history (busy)"
    );
}
