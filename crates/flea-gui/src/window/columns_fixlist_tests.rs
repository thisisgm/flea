//! FIX-LIST 1101 in Columns: a lane or divider press is blank space (EY-532); only a tagged row reserves the tag slot (EY-529).
use super::host;
use crate::columns::geometry::Slot;
use gpui::{point, px, MouseButton, MouseDownEvent};

/// EY-532: a right click on the divider at a column's right edge opens that column's background menu, never a row's.
#[test]
fn fixlist_ey532_a_right_click_on_the_divider_is_blank_space() {
    for (slot, path) in [(Slot::Active, "/srv/a/b/c"), (Slot::Parent, "/srv/a/b")] {
        let mut cx = host::context();
        let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
        host::change(&mut cx, handle, |shell, window, cx| {
            host::peek(
                shell,
                "/srv/a/b",
                &[("c", true), ("other", false)],
                false,
                window,
                cx,
            );
            host::settle(shell, window, cx);
            shell.panes[0].as_mut().unwrap().selected = vec![0];
        });
        host::frame(&mut cx, handle);
        host::change(&mut cx, handle, |shell, window, cx| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(slot).unwrap().borrow();
            let drawn = paint.drawn.as_ref().unwrap();
            // Row 1's height, one pixel inside the track's right edge, where the divider is drawn.
            let position = point(
                drawn.bounds.right() - px(1.),
                drawn.bounds.origin.y + px(1.5 * drawn.row_height),
            );
            drop(paint);
            shell.columns_pointer(
                0,
                &MouseDownEvent {
                    position,
                    button: MouseButton::Right,
                    click_count: 1,
                    ..Default::default()
                },
                window,
                cx,
            );
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, 0, "{slot:?}: the press moved the cursor");
            assert_eq!(
                host.selected,
                [0],
                "{slot:?}: the press changed the selection"
            );
            let captured = shell.menu.captured.as_ref().expect("a menu opened");
            assert!(
                captured.background,
                "{slot:?}: the divider opened a row menu"
            );
            assert_eq!(captured.path, path);
        });
    }
}

/// EY-529: a column with one tagged row keeps every untagged name at the full budget, as ColumnRow.qml reserves per row.
#[test]
fn fixlist_ey529_only_a_tagged_row_gives_its_name_budget_to_the_tag_slot() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    let long = "a_name_long_enough_to_be_cut_in_any_column_".repeat(3);
    host::change(&mut cx, handle, |shell, window, cx| {
        host::settle(shell, window, cx);
        let host = shell.panes[0].as_mut().unwrap();
        let mut tagged = host::row(&format!("{long}1"), false);
        tagged.tags = Some(vec!["Red".into(), "Blue".into()]);
        let plain = host::row(&format!("{long}2"), false);
        host.pane.held = std::rc::Rc::new(crate::list::pane::Held {
            listing: host.pane.held.listing,
            total: 2,
            start: 0,
            rows: vec![tagged, plain],
        });
    });
    host::frame(&mut cx, handle);
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let paint = columns.painters.get(Slot::Active).unwrap().borrow();
        let label = |index: usize| {
            paint
                .drawn_label(index)
                .expect("the row is drawn")
                .chars()
                .count()
        };
        assert!(
            shell.tag_slot(0, shell.panes[0].as_ref().unwrap().pane.held.listing),
            "the folder latched its tag slot"
        );
        let (tagged, plain) = (label(0), label(1));
        assert!(
            plain > tagged,
            "the untagged name kept {plain} cells, no more than the tagged name's {tagged}"
        );
    });
}
