//! EY-203: Grid PageUp and PageDown move half the visible rows in items, as v0.3.12's Focus.js:168 does.
use super::{
    join_tests::{call, frame, key, open},
    Shown,
};
use crate::{pane::Held, state::Shell, window::startup::View};
use gpui::{px, size, TestAppContext, VisualTestContext, WindowHandle};
use std::rc::Rc;

fn cursor(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> usize {
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().pane.cursor
        })
        .unwrap()
}

#[gpui::test]
fn ey203_grid_page_keys_move_half_the_visible_rows_in_items(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    handle
        .update(cx, |shell, _, _| {
            let held = &mut shell.panes[0].as_mut().unwrap().pane.held;
            *held = Rc::new(Held {
                listing: held.listing,
                total: 80,
                start: held.start,
                rows: held.rows[..80].to_vec(),
            });
        })
        .unwrap();
    // The scene's window: six columns and 20 or 21 visible rows, so floor(visibleRows / 2) is 10.
    let columns = |cx: &mut TestAppContext| {
        handle
            .update(cx, |shell, _, _| {
                let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
                grid.view.layout(Shown::All(80)).columns
            })
            .unwrap()
    };
    let visible = |cx: &mut TestAppContext| -> usize {
        call(cx, handle, "visibleRows", &[])
            .stdout
            .trim()
            .parse()
            .unwrap()
    };
    let resize = |cx: &mut TestAppContext, width: f32, height: f32| {
        let visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_resize(size(px(width), px(height)));
        frame(cx, handle);
    };
    let mut width = 0.;
    for candidate in (600..1600).map(|width| width as f32) {
        resize(cx, candidate, 700.);
        if columns(cx) == 6 {
            width = candidate;
            break;
        }
    }
    assert_eq!(columns(cx), 6, "the scene's 80 items sit in six columns");
    for height in (500..1000).map(|height| height as f32) {
        resize(cx, width, height);
        if (20..=21).contains(&visible(cx)) {
            break;
        }
    }
    let visible_rows = visible(cx);
    let pane_rows = handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().pane.visible_rows()
        })
        .unwrap();
    assert_eq!(
        visible_rows, pane_rows,
        "visibleRows answers the pane's own visible rows, ceil(list.height / file row)"
    );
    assert!(
        (20..=21).contains(&visible_rows),
        "v0.3.12 pages floor(visibleRows / 2) = 10 items only at 20 or 21 visible rows, got {visible_rows}"
    );

    key(cx, handle, "pagedown");
    assert_eq!(
        cursor(cx, handle),
        10,
        "PageDown from item 0 lands on item 10"
    );
    key(cx, handle, "end");
    assert_eq!(cursor(cx, handle), 79, "End lands on item 79");
    key(cx, handle, "pageup");
    assert_eq!(
        cursor(cx, handle),
        69,
        "PageUp from item 79 lands on item 69 of 80"
    );
}
