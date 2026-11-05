//! The Columns page: a shown column moves by its grip, and the cursor keeps its column as the order changes.
use super::panel::{Panel, Press};
use super::tests::{panel, press};
use flea::jsondoc::render;

#[test]
fn a_shown_column_dragged_by_its_grip_changes_the_draw_order() {
    let mut panel = panel(r#"{"columns":["name","mode","size","date"]}"#);
    panel.show_section("columns");
    let at = |panel: &Panel, id: &str| panel.rows.iter().position(|row| row.id == id).unwrap();
    let shown = |panel: &Panel| {
        render(panel.document().get("columns").unwrap())
            .split_whitespace()
            .collect::<String>()
    };
    // Mode dragged two rows down lands after Modified; the cursor follows it to its new row.
    panel.press(at(&panel, "column:mode"), Press::Drag(2));
    assert_eq!(shown(&panel), r#"["name","size","date","mode"]"#);
    assert_eq!(panel.rows[panel.cursor].id, "column:mode");
    // Past the top it stops first, and the name keeps its place ahead of every column.
    panel.press(at(&panel, "column:mode"), Press::Drag(-9));
    assert_eq!(shown(&panel), r#"["name","mode","size","date"]"#);
    // A hidden column has no place in the order to move from.
    panel.press(at(&panel, "column:kind"), Press::Drag(-1));
    assert_eq!(shown(&panel), r#"["name","mode","size","date"]"#);
}

#[test]
fn a_toggled_column_keeps_the_cursor_as_it_moves_among_the_hidden() {
    let mut panel = panel(r#"{"columns":["name","size","date"]}"#);
    panel.show_section("columns");
    panel.cursor = panel
        .rows
        .iter()
        .position(|row| row.id == "column:size")
        .unwrap();
    // Hidden, Size moves below Modified; a second press shows it again rather than hiding Modified.
    press(&mut panel, &["space"]);
    assert_eq!(panel.rows[panel.cursor].id, "column:size");
    press(&mut panel, &["space"]);
    let shown = render(panel.document().get("columns").unwrap());
    assert!(shown.contains("date"), "{shown}");
    assert!(shown.contains("size"), "{shown}");
}
