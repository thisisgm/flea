//! EY-203 for List: PageDown steps half the visible rows, on the key path that runs before a list frame is drawn.
use super::*;
use gpui::Entity;

fn mounted(cx: &mut TestAppContext) -> (Entity<Shell>, &mut gpui::VisualTestContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_resize(size(px(900.), px(600.)));
    cx.run_until_parked();
    (shell, cx)
}

// Focus.js:168 steps floor(visibleRows / 2) items, and Pane.qml:281 counts visibleRows as ceil(list.height / row).
#[gpui::test]
fn ey203_list_page_down_steps_half_the_visible_rows_before_a_frame(cx: &mut TestAppContext) {
    let (shell, cx) = mounted(cx);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().input.frame = None;
    });
    let visible = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.visible_rows()
    });
    cx.update(|window, cx| shell.update(cx, |shell, cx| super::key(shell, "pagedown", window, cx)));
    cx.run_until_parked();
    let cursor = shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor);
    assert!(visible > 2, "the fixture shows more than two rows");
    assert_eq!(
        cursor,
        (visible / 2).max(1),
        "PageDown from item 0 lands on half of {visible} visible rows, not a whole page"
    );
}
