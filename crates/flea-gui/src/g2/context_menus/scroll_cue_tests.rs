//! A long menu and a small window's Copy As flyout scroll with a half-row cue.
use super::*;

#[gpui::test]
fn sol_menu_order_a_long_menu_scrolls_with_a_half_row_cue(cx: &mut TestAppContext) {
    let (handle, _) = window(cx);
    with(cx, handle, |menu, env, window| {
        menu.open(Face::File, file_rows(), (10., 10.), SMALL, "a", env, window);
        let card = menu.card.as_ref().unwrap();
        assert!(
            fit::content_height(&card.entries) > card.view_h,
            "past the fold"
        );
        assert!(
            fit::cue_ok(&card.entries, card.content_y, card.view_h),
            "at rest the bottom edge cuts a row at half"
        );
        for _ in 0..40 {
            menu.act("cursorDown", None, "a", env, window);
        }
        let card = menu.card.as_ref().unwrap();
        let top = fit::row_top(&card.entries, card.cursor.unwrap()) - card.content_y;
        assert!(
            top >= 0 && top + fit::ROW <= card.view_h,
            "the cursor row is inside the view"
        );
        assert_eq!(
            card.entries[card.cursor.unwrap()].action,
            "trash",
            "Move to Trash is last, as the HIG file menu draws it"
        );
        let before = card.content_y;
        let (x, y) = (card.frame.x + 10., card.frame.y + card.frame.h / 2.);
        for _ in 0..40 {
            menu.wheel(x, y, 120);
        }
        let card = menu.card.as_ref().unwrap();
        assert!(
            card.content_y < before && card.cursor == Some(0),
            "the wheel steps the cursor back up a row a notch"
        );
        assert_eq!(card.frame.h, (card.view_h + 2 * PAD) as f32);
    });
}

#[gpui::test]
fn copy_as_in_a_small_window_shows_a_half_row_under_each_parent_fade(cx: &mut TestAppContext) {
    let (handle, menu) = window(cx);
    with(cx, handle, |menu, env, window| {
        // Text size 14 insets the menu 5 px, so the file menu's frame is 230 tall at 480x240.
        let env = Env { inset: 5., ..*env };
        let rows = listing_entries(&Context {
            row_mode: 0o100644,
            row_is_file: true,
            has_row: true,
            selection_count: 1,
            hidden: Vec::new(),
            ..Context::default()
        });
        menu.open(Face::File, rows, (4., 4.), SMALL, "a", &env, window);
        menu.open_submenu_for("copyAs", true, &env, window);
        let card = menu.card.as_ref().unwrap();
        assert_eq!(card.frame.h, 230., "the accepted frame keeps its height");
        let (top, bottom) = fit::edge_cuts(&card.entries, card.content_y, card.view_h);
        let label = |cut: Option<(usize, i32)>| {
            cut.map(|(index, shown)| (card.entries[index].action.to_string(), shown))
        };
        assert_eq!(
            (label(top), label(bottom)),
            (
                Some(("openTerminal".to_owned(), 16)),
                Some(("tags".to_owned(), 17))
            ),
            "half of Open in Terminal over Move To and half of Tags under Copy As"
        );
        assert!(fit::keeps_clear(
            &card.entries,
            card.cursor,
            card.content_y,
            card.view_h
        ));
    });
    draw(cx, handle);
    let state = read(&menu.borrow(), "menuState", &[]);
    assert!(
        state.contains("\"view\":218"),
        "the view is not trimmed: {state}"
    );
}
