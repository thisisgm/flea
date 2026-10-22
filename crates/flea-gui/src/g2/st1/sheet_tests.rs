//! tests/sheet-query.qml: the sheet's query reads back as typed, a dead menu row refuses Enter, and the pointer runs a result.
use super::seams::Request;
use super::sheet::{Sheet, Sources};
use super::tests::english;
use flea::ui_logic_st1::{
    sheet::{Decision, MenuEntry},
    Text,
};

fn typed(sheet: &mut Sheet, word: &str) {
    for letter in word.chars() {
        let text = letter.to_string();
        sheet.key(&text, &text);
    }
}

#[test]
fn typing_reads_back_whole_and_backspace_takes_one_character_at_a_time() {
    let mut sheet = Sheet::new("default", english(), Sources::default());
    typed(&mut sheet, "tag");
    assert_eq!(sheet.query, "tag");
    let steps: Vec<String> = (0..3)
        .map(|_| {
            sheet.key("backspace", "");
            sheet.query.clone()
        })
        .collect();
    assert_eq!(steps.join("|"), "ta|t|");
    assert!(
        sheet.requests.is_empty(),
        "the last one leaves the resting sheet open"
    );
}

fn permissions(disabled: bool) -> Sources {
    let entry = MenuEntry {
        label: Text::data("Permissions…"),
        action: "permissions".into(),
        disabled,
        reason: if disabled {
            Text::data("Nothing selected")
        } else {
            Text::Empty
        },
        ..Default::default()
    };
    Sources {
        menus: vec![entry],
        ..Sources::default()
    }
}

#[test]
fn a_live_menu_row_runs_through_the_menu_and_a_dead_one_refuses_enter() {
    let mut sheet = Sheet::new("default", english(), permissions(false));
    typed(&mut sheet, "perm");
    let at = sheet
        .ranked
        .iter()
        .position(|row| row.label.resolve(&sheet.catalog) == "Permissions…")
        .expect("Permissions is listed");
    assert!(!sheet.ranked[at].disabled);
    sheet.point(at, true);
    assert_eq!(
        sheet.decided,
        Some(Decision::Menu {
            menu_action: "permissions".into()
        })
    );
    assert_eq!(sheet.requests, [Request::Close]);
    let mut sheet = Sheet::new("default", english(), permissions(true));
    typed(&mut sheet, "perm");
    let at = sheet
        .ranked
        .iter()
        .position(|row| row.label.resolve(&sheet.catalog) == "Permissions…")
        .expect("Permissions is listed");
    assert!(sheet.ranked[at].disabled);
    assert_eq!(
        sheet.ranked[at].reason.resolve(&sheet.catalog),
        "Nothing selected"
    );
    sheet.point(at, true);
    assert_eq!(sheet.decided, None);
    assert!(
        sheet.requests.is_empty(),
        "Enter is refused and the sheet stays open"
    );
}

#[test]
fn the_pointer_highlights_a_result_and_scrolls_only_as_far_as_it_needs() {
    let mut sheet = Sheet::new("default", english(), Sources::default());
    typed(&mut sheet, "e");
    let count = sheet.ranked.len();
    assert!(count > 10, "{count}");
    sheet.point(count - 1, false);
    assert_eq!(sheet.cursor, count - 1);
    assert!(sheet.requests.is_empty(), "a hover runs nothing");
    sheet.reveal(250., 25.);
    assert_eq!(sheet.scroll, count as f32 * 25. - 250.);
    sheet.point(0, false);
    sheet.reveal(250., 25.);
    assert_eq!(sheet.scroll, 0.);
    sheet.point(count, true);
    assert_eq!(sheet.cursor, 0, "a row past the list is no row");
}

// checkTop: the card sits where the board puts it at 1100 by 700, stays inside a short window, and takes nothing from the sheet's state.
#[test]
fn the_card_takes_its_place_from_the_window_alone() {
    use super::sheet_view::{card_rect, SHEET_H, SHEET_W};
    assert_eq!(card_rect(1100., 700.), (236., 8., SHEET_W, SHEET_H));
    assert_eq!(card_rect(1100., 1400.), (236., 358., SHEET_W, SHEET_H));
    assert_eq!(card_rect(600., 400.), (8., 8., 584., 384.));
    assert_eq!(card_rect(10., 10.), (5., 5., 0., 0.));
}
