// 0401-settings-view-c and 0401-settings-hover: Highlight on hover, Show tabs as icons and the empty folder's two rows, in the boards' order, marks and defaults.
use super::*;
use crate::jsondoc::{parse, Json};
use crate::ui_logic_settings_pages::test_support::say;

fn view(text: &str) -> Vec<Row> {
    view_rows(&State::new(&parse(text).expect("fixture parses")))
}

fn row<'a>(rows: &'a [Row], id: &str) -> &'a Row {
    rows.iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("no row {id}"))
}

#[test]
fn the_four_rows_sit_after_today_and_before_dragging_with_the_boards_marks() {
    let rows = view("{}");
    let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    let today = ids.iter().position(|id| *id == "highlightToday").unwrap();
    assert_eq!(
        ids[today..today + 6],
        [
            "highlightToday",
            "highlightHover",
            "tabsAsIcons",
            "emptyFolderOmi",
            "emptyFolderRotate",
            "openFoldersWhileDragging"
        ]
    );
    let drawn: Vec<String> = [
        "highlightHover",
        "tabsAsIcons",
        "emptyFolderOmi",
        "emptyFolderRotate",
    ]
    .iter()
    .map(|id| {
        let r = row(&rows, id);
        format!("{}|{}|{}|{}", say(&r.label), r.glyph, r.on, say(&r.caption))
    })
    .collect();
    assert_eq!(
        drawn,
        [
            "Highlight on hover|pointer|false|",
            "Show tabs as icons|app-window|false|",
            "Show Omi in empty folders|bot|true|",
            "Rotate empty folder messages|bot|true|",
        ]
    );
}

#[test]
fn each_row_reads_its_own_key_and_turns_off_alone() {
    let omi_off = view(r#"{"emptyFolderOmi":false}"#);
    assert!(!row(&omi_off, "emptyFolderOmi").on && row(&omi_off, "emptyFolderRotate").on);
    assert!(row(&omi_off, "emptyFolderRotate").available);
    let rotate_off = view(r#"{"emptyFolderRotate":false}"#);
    assert!(row(&rotate_off, "emptyFolderOmi").on && !row(&rotate_off, "emptyFolderRotate").on);
    assert!(row(&rotate_off, "emptyFolderOmi").available);
    assert!(row(&view(r#"{"tabsAsIcons":true}"#), "tabsAsIcons").on);
    let hover = view(r#"{"highlightHover":true}"#);
    assert!(row(&hover, "highlightHover").on && !row(&hover, "highlightToday").on);
    assert!(!row(&view(r#"{"highlightToday":true}"#), "highlightHover").on);
}

#[test]
fn the_schema_ships_the_rows_as_041_draws_and_takes_only_booleans() {
    let fresh = crate::uistate::from_file("{}");
    for (key, on) in [
        ("highlightHover", false),
        ("tabsAsIcons", false),
        ("emptyFolderOmi", true),
        ("emptyFolderRotate", true),
    ] {
        assert_eq!(
            fresh.get(key).and_then(Json::as_bool),
            Some(on),
            "{key} ships {on}"
        );
        let good = parse(&format!(r#"{{"{key}":{}}}"#, !on)).unwrap();
        assert!(
            crate::uistate::patched(&fresh, &good).is_ok(),
            "{key} takes a boolean"
        );
        let bad = parse(&format!(r#"{{"{key}":"yes"}}"#)).unwrap();
        let message = crate::uistate::patched(&fresh, &bad).expect_err("a word is refused");
        assert!(message.contains(key), "{message} names {key}");
    }
}

#[test]
fn issue_434_row_matches_the_approved_cursor_board_and_search_finds_it() {
    let rows = view("{}");
    let at = rows
        .iter()
        .position(|row| row.id == "gridWrapRows")
        .unwrap();
    let wrap = &rows[at];
    assert_eq!(rows[at - 1].id, "wrapAtEnds");
    assert_eq!(rows[at + 1].id, "escapeUp");
    assert_eq!(say(&wrap.label), "Wrap at row ends");
    assert_eq!(say(&wrap.caption), "left and right in Grid");
    assert_eq!(wrap.glyph, "move-horizontal");
    assert!(!wrap.on);
    assert!(row(&view(r#"{"gridWrapRows":true}"#), "gridWrapRows").on);
    let sections = vec![(0, rows)];
    let found = crate::ui_logic_settings_pages::search::search(
        &sections,
        "row ends",
        &crate::ui_logic_settings_pages::test_support::english(),
    );
    assert_eq!(
        crate::ui_logic_settings_pages::search::first_target(&found, &sections),
        Some((0, at))
    );
}

#[test]
fn issue_434_row_wrap_survives_state_round_trip_and_requires_a_boolean() {
    let fresh = crate::uistate::from_file("{}");
    assert_eq!(
        fresh.get("gridWrapRows").and_then(Json::as_bool),
        Some(false)
    );
    let patch = parse(r#"{"gridWrapRows":true}"#).unwrap();
    let updated = crate::uistate::patched(&fresh, &patch).unwrap();
    let loaded = crate::uistate::from_file(&crate::jsondoc::render(&updated));
    assert!(row(&view_rows(&State::new(&loaded)), "gridWrapRows").on);
    assert_eq!(
        loaded.get("wrapAtEnds").and_then(Json::as_bool),
        Some(false)
    );
    for invalid in [r#"{"gridWrapRows":"true"}"#, r#"{"gridWrapRows":1}"#] {
        let message = crate::uistate::patched(&fresh, &parse(invalid).unwrap()).unwrap_err();
        assert!(message.contains("gridWrapRows"));
    }
}
