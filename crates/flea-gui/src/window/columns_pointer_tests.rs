use super::host;
use crate::{backend::Event, columns::geometry::Slot, window::columns::BackgroundMenu};
use gpui::{point, px, MouseButton, MouseDownEvent};
use std::{cell::RefCell, rc::Rc};

fn click(
    shell: &mut crate::state::Shell,
    slot: Slot,
    y: f32,
    button: MouseButton,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::state::Shell>,
) {
    let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
    let paint = columns.painters.get(slot).unwrap().borrow();
    let drawn = paint.drawn.as_ref().unwrap();
    let position = point(
        drawn.bounds.origin.x + drawn.bounds.size.width / 2.,
        drawn.bounds.origin.y + px(y),
    );
    drop(paint);
    shell.columns_pointer(
        0,
        &MouseDownEvent {
            position,
            button,
            click_count: 1,
            ..Default::default()
        },
        window,
        cx,
    );
}

#[test]
fn u09_live_columns_background_navigates_once_and_opens_only_after_its_own_rows() {
    for (slot, path) in [
        (Slot::Parent, "/srv/a/b"),
        (Slot::Active, "/srv/a/b/c"),
        (Slot::Child, "/srv/a/b/c/d01"),
    ] {
        let mut cx = host::context();
        let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
        let events = Rc::new(RefCell::new(Vec::<BackgroundMenu>::new()));
        let events_copy = Rc::clone(&events);
        let subscription = window
            .update(&mut cx, |_, _, cx| {
                cx.subscribe(&cx.entity(), move |_, _, event: &BackgroundMenu, _| {
                    events_copy.borrow_mut().push(event.clone())
                })
            })
            .unwrap();
        host::change(&mut cx, window, |shell, window, cx| {
            host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
            host::peek(shell, "/srv/a/b/c/d01", &[], false, window, cx);
        });
        host::frame(&mut cx, window);
        host::change(&mut cx, window, |shell, window, cx| {
            click(shell, slot, 300., MouseButton::Right, window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.place(), path);
            assert_eq!(host.nav.counters.lists, usize::from(slot != Slot::Active));
            if slot != Slot::Active {
                assert!(
                    events.borrow().is_empty(),
                    "a menu opened before the target's rows"
                );
                shell.receive(
                    0,
                    Event::Reply(flea::protocol::Reply::Listed {
                        path: path.into(),
                        total: 0,
                    }),
                    window,
                    cx,
                );
                shell.receive(
                    0,
                    Event::Reply(flea::protocol::Reply::Rows {
                        start: 0,
                        listing: 4,
                        rows: Vec::new(),
                    }),
                    window,
                    cx,
                );
            }
        });
        cx.run_until_parked();
        assert_eq!(events.borrow().len(), 1);
        assert_eq!(events.borrow()[0].path, path);
        assert_eq!(events.borrow()[0].at.1, 327.);
        drop(subscription);
    }
}

#[test]
fn u09_live_columns_failed_background_and_busy_or_blank_clicks_never_open_a_menu() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().nav.gate.editing = true;
        click(shell, Slot::Parent, 300., MouseButton::Right, window, cx);
        let host = shell.panes[0].as_mut().unwrap();
        assert!(host.nav.menus.background.target.is_empty());
        assert_eq!(host.nav.counters.lists, 0);
        host.nav.gate.editing = false;
        click(shell, Slot::Parent, 300., MouseButton::Right, window, cx);
        click(shell, Slot::Parent, 300., MouseButton::Right, window, cx);
        assert_eq!(
            shell.panes[0].as_ref().unwrap().nav.counters.lists,
            1,
            "busy input was not consumed"
        );
        shell.receive(
            0,
            Event::Refused {
                operation: "read".into(),
                path: "/srv/a/b".into(),
                message: "Permission denied".into(),
                mode: 0,
            },
            window,
            cx,
        );
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.nav.menus.background.target.is_empty());
        assert!(host.nav.background_at.is_none());
    });
    let (root, _requests) = host::open(&mut cx, "/");
    host::change(&mut cx, root, host::settle);
    host::frame(&mut cx, root);
    host::change(&mut cx, root, |shell, window, cx| {
        click(shell, Slot::Parent, 300., MouseButton::Right, window, cx);
        assert_eq!(shell.panes[0].as_ref().unwrap().nav.counters.lists, 0);
    });
}
