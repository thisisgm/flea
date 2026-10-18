use super::*;

#[test]
fn parity_checker_rejects_a_wrong_bound_key() {
    let text = include_str!("parity.tsv");
    check_rows(text, &Map::load());
    let mutant = text.replace("mgr\t\"q\"\t\"quit\"", "mgr\t\"Q\"\t\"quit\"");
    assert_ne!(mutant, text);
    let result = std::panic::catch_unwind(|| check_rows(&mutant, &Map::load()));
    assert!(result.is_err(), "the parity checker accepted bound q changed to upstream Q");
}

#[test]
fn parity_checker_rejects_an_unsupported_key_override() {
    check_rows(include_str!("parity.tsv"), &Map::load());
    let text = include_str!("../../keys.toml");
    let mutant = text.replace("action = \"unsupported:Content search is unavailable; s opens built-in filename search\"", "action = \"cursorDown\"");
    assert_ne!(mutant, text);
    let map = Map::parse(&mutant);
    assert_eq!(map.action(&Key::character('S', ""), "yazi"), "cursorDown");
    let result = std::panic::catch_unwind(|| check_rows(include_str!("parity.tsv"), &map));
    assert!(result.is_err(), "the parity checker accepted an unsupported key bound to cursorDown");
}

#[test]
fn parity_checker_rejects_inert_dialog_keys() {
    let text = include_str!("parity.tsv");
    check_rows(text, &Map::load());
    let lines: Vec<_> = text.lines().collect();
    let mut checked = 0;
    for (index, line) in lines.iter().enumerate() {
        let mut row: Vec<_> = line.split('\t').map(str::to_owned).collect();
        if row.len() != 8 || row[6] == "unsupported" || !matches!(row[3].as_str(), "confirm" | "completion") { continue; }
        row[4] = "[\"<F24>\"]".into();
        // Keep metadata valid so only execution can reject this key.
        row[6] = "different".into();
        let mut mutant: Vec<_> = lines.iter().map(|line| (*line).to_owned()).collect();
        mutant[index] = row.join("\t");
        let result = std::panic::catch_unwind(|| check_rows(&mutant.join("\n"), &Map::load()));
        assert!(result.is_err(), "the parity checker accepted F24 for {} {}", row[3], row[1]);
        checked += 1;
    }
    assert_eq!(checked, 8);
}

#[test]
fn parity_checker_rejects_inert_editor_motion() {
    let text = include_str!("parity.tsv");
    check_rows(text, &Map::load());
    let lines: Vec<_> = text.lines().collect();
    let mut checked = 0;
    for (index, line) in lines.iter().enumerate() {
        let mut row: Vec<_> = line.split('\t').map(str::to_owned).collect();
        if row.len() != 8 || row[3] != "editor" || !matches!(row[5].as_str(), "move 1" | "move eol") { continue; }
        row[4] = "[\"<F24>\"]".into();
        // Keep metadata valid so only execution can reject this key.
        row[6] = "different".into();
        let mut mutant: Vec<_> = lines.iter().map(|line| (*line).to_owned()).collect();
        mutant[index] = row.join("\t");
        let result = std::panic::catch_unwind(|| check_rows(&mutant.join("\n"), &Map::load()));
        assert!(result.is_err(), "the parity checker accepted F24 for {} {}", row[5], row[1]);
        checked += 1;
    }
    assert_eq!(checked, 6);
}

#[test]
fn parity_checker_rejects_inert_editor_close() {
    let text = include_str!("parity.tsv");
    check_rows(text, &Map::load());
    let lines: Vec<_> = text.lines().collect();
    let mut checked = 0;
    for (index, line) in lines.iter().enumerate() {
        let mut row: Vec<_> = line.split('\t').map(str::to_owned).collect();
        if row.len() != 8 || row[3] != "editor" || !matches!(row[5].as_str(), "close" | "escape" | "close --submit") { continue; }
        row[4] = "[\"<F24>\"]".into();
        // Keep metadata valid so only execution can reject this key.
        row[6] = "different".into();
        let mut mutant: Vec<_> = lines.iter().map(|line| (*line).to_owned()).collect();
        mutant[index] = row.join("\t");
        let result = std::panic::catch_unwind(|| check_rows(&mutant.join("\n"), &Map::load()));
        assert!(result.is_err(), "the parity checker accepted F24 for {} {}", row[5], row[1]);
        checked += 1;
    }
    assert_eq!(checked, 4);
}
