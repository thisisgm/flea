use super::*;

// A modal card's press never reaches the Columns rows behind it; the same press without the card moves the cursor.
#[test]
fn a_press_on_columns_under_the_settings_card_leaves_the_cursor() {
    let (mut cx, handle, _received) = open(View::Columns);
    reset(&mut cx, handle);
    let at = update(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let paint = host
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(Slot::Active)
            .unwrap()
            .borrow();
        let bounds = paint.drawn.as_ref().unwrap().bounds;
        point(
            bounds.left() + px(40.),
            bounds.top() + px(crate::list::pane::ROW_HEIGHT * 2.5),
        )
    });
    let row = column_row(&mut cx, handle, at);
    press(&mut cx, handle, at, MouseButton::Left);
    update(&mut cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().pane.cursor,
            row,
            "control: the bare press moves the cursor"
        );
    });
    reset(&mut cx, handle);
    update(&mut cx, handle, |shell, window, cx| {
        shell.open_settings(window, cx)
    });
    frame(&mut cx, handle);
    press(&mut cx, handle, at, MouseButton::Left);
    update(&mut cx, handle, |shell, _, cx| {
        assert!(shell.input_blocked(cx), "the settings card holds input");
        assert_eq!(
            shell.panes[0].as_ref().unwrap().pane.cursor,
            0,
            "a press under the card changed Columns"
        );
    });
}
