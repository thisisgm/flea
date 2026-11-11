//! Columns rows draw at Row density, so the thumbnail asks follow that pitch, not the fixed list row.
use super::*;

fn columns_asks_follow_density(
    cx: &mut TestAppContext,
    density: crate::list::Density,
    name: &str,
    stock_size: bool,
) {
    let view = View::Columns;
    let (handle, mut lines) = open(cx, view, "{}");
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let mut list = host.list.borrow_mut();
            list.density = density;
            // Stock Omarchy text size 12 (GM runs 14, the test face's base).
            if stock_size {
                let mut face = crate::list::Typeface::omarchy_default();
                face.base = px(12.);
                face.body = px(12.);
                face.body_small = px(11.);
                face.caption = px(10.);
                list.set_typeface(face);
            }
            host.pane.set_row_height(list.file_metrics().row);
        })
        .unwrap();
    for _ in 0..2 {
        frame(cx, handle);
        handle.update(cx, answer_peeks).unwrap();
        cx.run_until_parked();
    }
    let pitch = read(cx, handle, |shell| {
        shell.panes[0].as_ref().unwrap().columns_row_height()
    });
    assert_ne!(pitch, crate::pane::ROW_HEIGHT, "{name} draws another pitch");
    first_settle(cx, handle, &mut lines);
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().pane.scroll_y = 3_100.
        })
        .unwrap();
    step(cx, handle);
    settle(cx, handle, 120);
    let moved = drained(&mut lines);
    let now: Vec<usize> = read(cx, handle, |shell| drawn(shell, view))
        .into_iter()
        .map(|(row, _)| row)
        .collect();
    assert!(
        !now.is_empty(),
        "{name}: the track draws rows at scroll 3100"
    );
    let mut wanted = now.clone();
    wanted.retain(|row| asked(&moved).contains(row));
    assert_eq!(
        wanted,
        now,
        "{name} at pitch {pitch}: every Columns row in view is asked for its picture; asked {:?}",
        asked(&moved)
    );
}

#[gpui::test]
fn columns_comfortable_rows_in_view_are_asked(cx: &mut TestAppContext) {
    columns_asks_follow_density(cx, crate::list::Density::Comfortable, "Comfortable", false);
}

#[gpui::test]
fn columns_tight_rows_in_view_are_asked(cx: &mut TestAppContext) {
    columns_asks_follow_density(cx, crate::list::Density::Tight, "Tight", false);
}

#[gpui::test]
fn columns_default_compact_at_stock_text_size_rows_in_view_are_asked(cx: &mut TestAppContext) {
    columns_asks_follow_density(
        cx,
        crate::list::Density::Compact,
        "Compact at text size 12",
        true,
    );
}
