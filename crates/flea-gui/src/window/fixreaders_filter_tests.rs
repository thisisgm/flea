//! FIX-HARNESS-READERS: row readers under a standing filter (EY-188 part 1, ruling 126).
use super::*;
use crate::window::dispatch::test_shell::land;
use crate::window::u07_tests::read;
use flea::protocol::RowFacts;

// The rows the filter answer below keeps, at view positions 0 and 1.
const KEPT: [usize; 2] = [5, 7];
// The landed folder's size, and the listing its answer opens after the fixture's own listing 2.
const TOTAL: usize = 10;
const LISTING: usize = 3;
// The itemFor readers EY-188 found answering nothing under a filter.
const EY188_READERS: [&str; 8] = [
    "rowAt",
    "rowNamePx",
    "rowNameText",
    "rowGlyph",
    "rowSizeText",
    "rowDateColor",
    "rowCellColor",
    "rowCellOverflow",
];

// EY-188: Ipc.qml's itemFor readers take a view position, so under a filter keeping r5 and r7 view 0 is r5; visibleRowName keeps the listing row.
#[gpui::test]
fn fixreaders_ey188_item_readers_take_the_view_position_under_a_filter(cx: &mut TestAppContext) {
    let handle = open(cx);
    frame(cx, handle);
    let shell = handle.update(cx, |root, _, _| root.shell.clone()).unwrap();
    let rows = (0..TOTAL)
        .map(|at| RowFacts {
            name: format!("r{at}.txt"),
            ..RowFacts::default()
        })
        .collect();
    land(
        &shell,
        &mut VisualTestContext::from_window(handle.into(), cx),
        "/files",
        rows,
        LISTING,
    );
    // The filter's first kept row takes the cursor, so it holds it before the filter too and both reads draw the same ink.
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = KEPT[0]
    });
    frame(cx, handle);
    // Unfiltered, view and listing row agree, so these are what rows 5 and 7 draw.
    let unfiltered = KEPT.map(|row| {
        let at = row.to_string();
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        EY188_READERS.map(|name| {
            let value = read(&shell, cx, name, &[&at]);
            assert!(
                !value.is_empty() && value != "-1" && value != "loading",
                "{name} {row}: {value:?}"
            );
            value
        })
    });
    let kept = KEPT.map(|at| at.to_string()).join(",");
    edit(cx, handle, |shell, _, cx| {
        filter(shell, "r", TOTAL, LISTING, &kept, cx)
    });
    frame(cx, handle);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    let drawn = shell.read_with(&visual, |shell, _| {
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        list.visible()
            .iter()
            .map(|row| (row.view, row.index))
            .collect::<Vec<_>>()
    });
    assert_eq!(drawn, [(0, KEPT[0]), (1, KEPT[1])]);
    let cx = &mut visual;
    for (view, drawn) in unfiltered.iter().enumerate() {
        let at = view.to_string();
        for (name, value) in EY188_READERS.iter().zip(drawn) {
            assert_eq!(
                &read(&shell, cx, name, &[&at]),
                value,
                "{name} at view {view}"
            );
        }
    }
    // View 5 is past the two matches, so no delegate stands there, though listing row 5 is drawn.
    assert_eq!(read(&shell, cx, "rowAt", &["5"]), "loading");
    assert_eq!(read(&shell, cx, "rowNamePx", &["5"]), "-1");
    // visibleRowName goes through visibleItemFor, a listing row: row 0 is hidden, row 5 is drawn.
    assert_eq!(read(&shell, cx, "visibleRowName", &["0"]), "");
    assert_eq!(read(&shell, cx, "visibleRowName", &["5"]), "r5.txt");
}
