use super::*;

#[test]
fn parity_checker_rejects_a_wrong_bound_key() {
    let text = include_str!("parity.tsv");
    check_rows(text, &Map::load());
    let mutant = text.replace("mgr\t\"q\"\t\"quit\"", "mgr\t\"Q\"\t\"quit\"");
    assert_ne!(mutant, text);
    assert!(std::panic::catch_unwind(|| check_rows(&mutant, &Map::load())).is_err());
}

#[test]
fn parity_checker_rejects_an_unsupported_key_override() {
    let text = include_str!("../../keys.toml");
    let legacy = text.replace(
        "action = \"unsupported:Content search is unavailable; s opens built-in filename search\"",
        "action = \"cursorDown\"",
    );
    let mutant = if legacy == text {
        text.replace(
            "key = \"S\"\naction = \"searchContents\"",
            "key = \"S\"\naction = \"cursorDown\"",
        )
    } else {
        legacy
    };
    assert_ne!(mutant, text);
    let map = Map::parse(&mutant);
    assert_eq!(map.action(&Key::character('S', ""), "yazi"), "cursorDown");
    let row = include_str!("parity.tsv")
        .lines()
        .find(|line| line.contains("\tyazi-059\t"))
        .unwrap();
    let row: Vec<_> = row.split('\t').collect();
    check_row(&row, &Map::load());
    assert!(std::panic::catch_unwind(|| check_row(&row, &map)).is_err());
    assert!(std::panic::catch_unwind(|| listing_checks::listing_action(
        &sequence(row[4]),
        "searchContents",
        "yazi",
        &map
    ))
    .is_err());
}

#[test]
fn parity_checker_runs_the_live_check_for_every_listing_and_panel_row() {
    // Rows of parity.tsv binding a yazi key per context, TUI17 live rows included, so a dropped row is caught.
    const LISTING_ROWS: usize = 88;
    const PANEL_ROWS: usize = 18;
    let map = Map::load();
    let counts = || {
        (
            listing_checks::LISTING_CHECKS.with(std::cell::Cell::get),
            listing_checks::PANEL_CHECKS.with(std::cell::Cell::get),
        )
    };
    let (mut listing, mut panel) = (0, 0);
    for line in include_str!("parity.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line.split('\t').collect();
        if row[0] == "flea" || row[6] == "unsupported" || !matches!(row[3], "listing" | "panel") {
            continue;
        }
        let (listing_before, panel_before) = counts();
        check_row(&row, &map);
        let expected = if row[3] == "listing" {
            listing += 1;
            (listing_before + 1, panel_before)
        } else {
            panel += 1;
            (listing_before, panel_before + 1)
        };
        assert_eq!(
            counts(),
            expected,
            "{} {} skipped its own live check",
            row[3],
            row[1]
        );
    }
    assert_eq!((listing, panel), (LISTING_ROWS, PANEL_ROWS));
}

// Each real row passes the row checker, and the same row moved to an inert F24 with valid metadata fails it.
fn reject_inert_keys(keep: fn(&[&str]) -> bool, expected: usize) {
    let map = Map::load();
    let mut checked = 0;
    for line in include_str!("parity.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line.split('\t').collect();
        if row[0] == "flea" || row[6] == "unsupported" || !keep(&row) {
            continue;
        }
        check_row(&row, &map);
        let mut mutant = row.clone();
        mutant[4] = "[\"<F24>\"]";
        mutant[6] = "different";
        assert!(
            std::panic::catch_unwind(|| check_row(&mutant, &map)).is_err(),
            "F24 accepted for {} {}",
            row[3],
            row[1]
        );
        checked += 1;
    }
    assert_eq!(checked, expected);
}

#[test]
fn parity_checker_rejects_inert_dialog_keys() {
    reject_inert_keys(|row| matches!(row[3], "confirm" | "completion"), 10);
}

#[test]
fn confirm_notes_claim_only_the_focus_the_dialog_keeps() {
    // dispatch keeps Cancel or Delete focused through Up, Down, k and j, so a note naming one button is false.
    const FOCUS_KEEPING_ROWS: usize = 4;
    let notes: Vec<_> = include_str!("parity.tsv")
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(|row| {
            row[0] == "confirm"
                && (row[5].starts_with("scroll") || matches!(row[1], "\"k\"" | "\"j\""))
        })
        .map(|row| row[7])
        .collect();
    assert_eq!(notes.len(), FOCUS_KEEPING_ROWS);
    for note in notes {
        assert!(note.contains("without moving button focus"), "{note}");
    }
}

#[test]
fn bound_appendix_notes_never_call_their_action_unavailable() {
    // candidate::appendix runs every bound Flea row's handler, so a note saying the action is missing or unavailable is stale.
    let rows: Vec<_> = include_str!("parity.tsv")
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(|row| row[0] == "flea" && row[6] == "bound")
        .collect();
    assert!(!rows.is_empty());
    for row in rows {
        // Whole words only, so a word such as dismissing never reads as missing.
        let words = format!(
            " {} ",
            row[7]
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        );
        assert!(
            ![" unavailable ", " missing ", " has no ", " no tui "]
                .iter()
                .any(|phrase| words.contains(phrase)),
            "{} {}: {}",
            row[8],
            row[9],
            row[7]
        );
    }
}

#[test]
fn parity_notes_never_repeat_their_description() {
    // A note joins the description to a group note, so a stop doubled after the description or a phrase description given twice is a join artifact.
    for row in include_str!("parity.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<_>>())
    {
        // Column 3 is JSON, a string or an array for a yazi command chain, and only a string is prose a note can repeat.
        let Json::Str(description) = jsondoc::parse(row[2])
            .unwrap_or_else(|error| panic!("{} {}: column 3 {}: {error}", row[8], row[9], row[2]))
        else {
            continue;
        };
        let stem = description.strip_suffix('.').unwrap_or(&description);
        assert!(
            stem.is_empty() || !row[7].contains(&format!("{stem}..")),
            "{} {}: {}",
            row[8],
            row[9],
            row[7]
        );
        assert!(
            !description.contains(' ') || row[7].matches(description.as_str()).count() < 2,
            "{} {}: {}",
            row[8],
            row[9],
            row[7]
        );
    }
}

#[test]
fn parity_checker_rejects_inert_editor_motion() {
    reject_inert_keys(
        |row| row[3] == "editor" && matches!(row[5], "move 1" | "move eol"),
        6,
    );
}

#[test]
fn parity_checker_rejects_inert_editor_close() {
    reject_inert_keys(
        |row| row[3] == "editor" && matches!(row[5], "close" | "escape" | "close --submit"),
        4,
    );
}

#[test]
fn parity_checker_rejects_inert_panel_motion() {
    reject_inert_keys(|row| row[3] == "panel" && row[5] == "cursorDown", 5);
    let map = Map::load();
    for properties in [false, true] {
        listing_checks::panel(&[key("<Down>")], "cursorDown", properties, &map);
        assert!(std::panic::catch_unwind(|| listing_checks::panel(
            &[key("<F24>")],
            "cursorDown",
            properties,
            &map
        ))
        .is_err());
    }
}
