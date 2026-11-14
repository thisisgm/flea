use super::{Frame, PaneInput};
use gpui::{point, px, size, Bounds};

/// A 100-row list of 20px rows seen through a 200px viewport, so ten rows show at a time.
fn input(keep_cursor_on_scroll: bool) -> PaneInput {
    PaneInput {
        frame: Some(Frame {
            bounds: Bounds::new(point(px(0.), px(0.)), size(px(800.), px(200.))),
            row: 20.,
            lane: 14.,
            content: 2_000.,
            scroll: 0.,
            listing: 1,
            total: 100,
            columns: 1,
            cell_width: 786.,
            grow: Default::default(),
            cell_origin: (0., 0.),
        }),
        keep_cursor_on_scroll,
        ..PaneInput::default()
    }
}

#[test]
fn a_scroll_past_the_cursor_pulls_it_into_the_viewport_by_default() {
    let mut input = input(false);
    let mut cursor = 0;
    input.scrolled(1_800., &mut cursor);
    assert_eq!(
        cursor, 90,
        "the upstream default keeps the cursor on the first visible row"
    );
}

#[test]
fn scroll_keeps_cursor_leaves_the_cursor_on_its_row_off_screen() {
    let mut input = input(true);
    let mut cursor = 0;
    input.scrolled(1_800., &mut cursor);
    assert_eq!(
        cursor, 0,
        "with scrollKeepsCursor the scroll moves only the viewport"
    );
    assert!(
        input.pulled.is_none(),
        "no pull is recorded, so a later extend starts from the cursor itself"
    );
}
