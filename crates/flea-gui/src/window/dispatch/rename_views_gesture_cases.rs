//! Gesture ownership while a pending rename refusal grows its row.
use super::*;

// The List's live gesture: whether a band is up, the row a press holds, and the selection.
fn gesture(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (bool, Option<usize>, Vec<usize>) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (
            host.input.band.is_some(),
            host.input.press.as_ref().map(|press| press.row),
            host.selected.clone(),
        )
    })
}

// The rows' height in the view: the List's own metrics or the Columns track's row.
fn row_height(shell: &Entity<Shell>, cx: &mut VisualTestContext, view: View) -> f32 {
    match view {
        View::Columns => crate::list::pane::ROW_HEIGHT,
        _ => shell.read_with(cx, |shell, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow()
                .drawn
                .metrics
                .row
        }),
    }
}

// An explicit retry is pending when a ground drag begins; its refusal grows the row mid-drag.
pub(super) fn band_drag_survives_a_refusal(cx: &mut TestAppContext, view: View) {
    let (shell, cx, mut lines) = refused_clash(cx, view);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        sent(&mut lines),
        [CLASH_SEND],
        "{view:?}: the retry is pending"
    );
    draw(cx);
    let row = row_height(&shell, cx, view);
    let bounds = clip(&shell, cx, view);
    // Below all four rows, with or without the band's extra height.
    let start = point(
        bounds.origin.x + px(40.),
        bounds.origin.y + px(4. * row + 40.),
    );
    cx.simulate_mouse_move(start, None, Modifiers::default());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(
        sent(&mut lines),
        Vec::<String>::new(),
        "{view:?}: the press cannot resend the pending retry"
    );
    assert!(gesture(&shell, cx).0, "{view:?}: the press starts a band");
    let mut at = start;
    for step in 1..=3 {
        at = start + point(px(20.), px(-40. * step as f32));
        cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::default());
        draw(cx);
    }
    receive(&shell, cx, clash_refusal());
    draw(cx);
    assert!(shell.read_with(cx, |shell, _| {
        shell
            .rename
            .edit
            .as_ref()
            .unwrap()
            .line
            .error_area
            .is_some()
    }));
    assert!(
        gesture(&shell, cx).0,
        "{view:?}: the refusal's band must not cancel the band drag"
    );
    for step in 4..=5 {
        at = start + point(px(20.), px(-40. * step as f32));
        cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::default());
        draw(cx);
    }
    let (band, _, selected) = gesture(&shell, cx);
    assert!(band, "{view:?}: the band survives every frame of the drag");
    assert!(
        selected.contains(&2) && selected.contains(&3),
        "{view:?}: the band selects the rows it crosses: {selected:?}"
    );
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    draw(cx);
    assert!(sent(&mut lines)
        .iter()
        .all(|line| flea::json::field_str(line, "c").as_deref() != Some("rename")));
    close(&shell, cx);
}

// A press on a row, while the commit's send is pending, keeps its press when the refusal raises the band.
pub(super) fn row_press_survives_a_refusal(cx: &mut TestAppContext, view: View) {
    let (shell, cx, mut lines) = refused_clash(cx, view);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        sent(&mut lines),
        [CLASH_SEND],
        "{view:?}: the retry is pending"
    );
    draw(cx);
    let row = row_height(&shell, cx, view);
    let bounds = clip(&shell, cx, view);
    // The first row, above the band's extra height.
    let press = point(bounds.origin.x + px(40.), bounds.origin.y + px(row / 2.));
    cx.simulate_mouse_move(press, None, Modifiers::default());
    cx.simulate_mouse_down(press, MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(
        gesture(&shell, cx).1,
        Some(0),
        "{view:?}: the press holds alpha"
    );
    receive(&shell, cx, clash_refusal());
    draw(cx);
    draw(cx);
    assert!(shell.read_with(cx, |shell, _| {
        shell
            .rename
            .edit
            .as_ref()
            .unwrap()
            .line
            .error_area
            .is_some()
    }));
    assert_eq!(
        gesture(&shell, cx).1,
        Some(0),
        "{view:?}: the refusal's band must not cancel the row press"
    );
    cx.simulate_mouse_up(press, MouseButton::Left, Modifiers::default());
    draw(cx);
    assert!(sent(&mut lines)
        .iter()
        .all(|line| flea::json::field_str(line, "c").as_deref() != Some("rename")));
    close(&shell, cx);
}
