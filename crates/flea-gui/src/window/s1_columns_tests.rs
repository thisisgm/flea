//! EY-571: waiting child menus follow Finder rule 5 without navigating.
use crate::{columns::geometry::Slot, window::columns_tests::host};
use gpui::{point, px, MouseButton, MouseDownEvent};
// EY-571: once the hold shows the cursor's folder still loading, a right click on its empty space opens that folder's
// background menu at the pointer, and the columns stay where they are (Finder rule 5, EY-379).
#[test]
fn s1_right_click_in_a_waiting_child_opens_its_background_menu() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/rows");
    host::change(&mut cx, window, |shell, window, cx| {
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        assert!(shell.panes[0]
            .as_mut()
            .unwrap()
            .pane
            .move_cursor(crate::list::pane::Move::End));
        shell.sync_columns(window, cx);
    });
    // Drive the timer's production state update after the pane's hold expires.
    let hold = std::time::Duration::from_millis(crate::pane::swap::HOLD_MS + 1);
    host::advance(&mut cx, window, hold);
    host::change(&mut cx, window, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        let now = host.pane.now();
        super::super::with_area(host, |columns, pane, ports| {
            columns.timer = None;
            columns.area.tick(&pane, ports, now);
        });
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        let columns = host.columns.as_ref().unwrap();
        assert_eq!(columns.area.host.shown.child, "/rows/d02");
        assert!(columns.area.host.fallback_at.is_none(), "the hold ended");
        let child = columns
            .area
            .tracks()
            .iter()
            .find(|view| view.slot == Slot::Child)
            .unwrap()
            .clone();
        assert!(!child.enabled, "d02 is still loading");
        let before = host.nav.counters.lists;
        let paint = columns.painters.get(Slot::Child).unwrap().borrow();
        let drawn = paint.drawn.as_ref().expect("the waiting child is drawn");
        let position = point(
            drawn.bounds.origin.x + drawn.bounds.size.width / 2.,
            drawn.bounds.origin.y + px(300.),
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
        assert_eq!(host.pane.place(), "/rows", "the columns stay");
        assert_eq!(host.nav.counters.lists, before);
        let captured = shell
            .menu
            .captured
            .as_ref()
            .expect("the waiting child owns a menu");
        assert_eq!(captured.path, "/rows/d02");
        assert!(captured.background);
    });
}
