// 0401-settings-view-c: Show tabs as icons and the empty folder's two rows, in the board's order, marks and defaults.
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
fn the_three_rows_sit_after_today_and_before_dragging_with_the_boards_marks() {
    let rows = view("{}");
    let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    let today = ids.iter().position(|id| *id == "highlightToday").unwrap();
    assert_eq!(
        ids[today..today + 5],
        [
            "highlightToday",
            "tabsAsIcons",
            "emptyFolderOmi",
            "emptyFolderRotate",
            "openFoldersWhileDragging"
        ]
    );
    let drawn: Vec<String> = ["tabsAsIcons", "emptyFolderOmi", "emptyFolderRotate"]
        .iter()
        .map(|id| {
            let r = row(&rows, id);
            format!("{}|{}|{}|{}", say(&r.label), r.glyph, r.on, say(&r.caption))
        })
        .collect();
    assert_eq!(
        drawn,
        [
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
}

#[test]
fn the_schema_ships_the_rows_as_041_draws_and_takes_only_booleans() {
    let fresh = crate::uistate::from_file("{}");
    for (key, on) in [
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
