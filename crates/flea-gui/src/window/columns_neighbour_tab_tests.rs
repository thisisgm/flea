//! Tap.js tappedColumn: a middle tap on a peeked folder opens that folder in a new tab, through the real pointer route.
use super::host;
use crate::columns::{geometry::Slot, TrackRows};
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext};

#[test]
fn pb_tabs_neighbour_middle_tap_opens_only_a_folder_in_a_new_tab() {
    // (row y in the parent track, row it lands on, tabs after, path of the current tab)
    for (y, name, count, path) in [
        (45., "other", 2, "/srv/a/b/other"),
        (76., "notes.txt", 1, "/srv/a/b/c"),
    ] {
        let mut cx = host::context();
        let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
        host::change(&mut cx, window, |shell, window, cx| {
            host::peek(
                shell,
                "/srv/a/b",
                &[("c", true), ("other", true), ("notes.txt", false)],
                false,
                window,
                cx,
            );
            host::settle(shell, window, cx);
        });
        host::frame(&mut cx, window);
        let position = window
            .update(&mut cx, |shell, _, _| {
                let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
                let paint = columns.painters.get(Slot::Parent).unwrap().borrow();
                let drawn = paint.drawn.as_ref().unwrap();
                // The pointer's own row math, so a miss past the last row cannot pass for the file arm.
                let at = drawn.grow.position_at(y + drawn.scroll, drawn.row_height);
                let hit = match &drawn.view().rows {
                    TrackRows::Peek(rows) => drawn.row_at(at).and_then(|index| rows.get(index)),
                    _ => None,
                };
                assert_eq!(hit.map(|row| row.name.as_str()), Some(name), "row at {y}");
                point(
                    drawn.bounds.origin.x + drawn.bounds.size.width / 2.,
                    drawn.bounds.origin.y + px(y),
                )
            })
            .unwrap();
        let mut vcx = VisualTestContext::from_window(window.into(), &cx);
        vcx.simulate_mouse_move(position, None, Modifiers::default());
        vcx.simulate_mouse_down(position, MouseButton::Middle, Modifiers::default());
        vcx.simulate_mouse_up(position, MouseButton::Middle, Modifiers::default());
        vcx.run_until_parked();
        host::change(&mut vcx, window, |shell, _, _| {
            let tabs = &shell.strip.tabs[0];
            assert_eq!(tabs.count(), count, "row at {y}");
            assert_eq!(tabs.current(), count - 1);
            let paths: Vec<_> = tabs
                .tabs
                .iter()
                .flat_map(|tabs| &tabs.items)
                .map(|tab| tab.path.as_str())
                .collect();
            assert_eq!(paths.last().copied().unwrap_or("/srv/a/b/c"), path);
            assert_eq!(paths.first().copied().unwrap_or("/srv/a/b/c"), "/srv/a/b/c");
        });
    }
}
