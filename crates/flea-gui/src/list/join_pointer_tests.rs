//! Real header pointer/key delivery, including a lost grab and a listing that becomes busy.
use super::{ask, frame, open};
use crate::{
    list::{bench, columns::Column, pane::Held},
    state::Shell,
};
use gpui::{
    point, px, Modifiers, MouseButton, MouseDownEvent, Point, TestAppContext, VisualTestContext,
    WindowHandle,
};
use std::rc::Rc;

fn edge(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    column: Column,
) -> Point<gpui::Pixels> {
    handle
        .update(cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            assert!(list.header.drawn.frames > 0, "HeaderElement is not mounted");
            let header = list.header.drawn.bounds;
            point(
                header.left() + px(list.header.drawn.layout.cell(column).unwrap().x),
                header.center().y,
            )
        })
        .unwrap()
}

fn stored(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    column: Column,
) -> (Option<u32>, usize) {
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            (
                host.list.borrow().view.stored.get(column),
                host.nav.writer.borrow().sent,
            )
        })
        .unwrap()
}

#[test]
fn u04_pointer_drag_previews_header_and_writes_once_on_release() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    let start = edge(&mut cx, handle, Column::Size);
    let before = handle
        .update(&mut cx, |shell, _, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow()
                .drawn
                .layout
                .cell(Column::Size)
                .unwrap()
                .width
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(start, None, Modifiers::default());
    vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    let finish = start + point(px(-40.), px(60.));
    vcx.simulate_mouse_move(finish, Some(MouseButton::Left), Modifiers::default());
    frame(&mut vcx, handle);
    handle
        .update(&mut vcx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let list = host.list.borrow();
            assert_eq!(
                list.header.drawn.layout.cell(Column::Size).unwrap().width,
                before + 40.
            );
            assert_eq!(list.drawn.layout.cell(Column::Size).unwrap().width, before);
            assert_eq!(host.nav.writer.borrow().sent, 0);
        })
        .unwrap();
    vcx.simulate_mouse_up(finish, MouseButton::Left, Modifiers::default());
    frame(&mut vcx, handle);
    assert_eq!(
        stored(&mut vcx, handle, Column::Size),
        (Some((before + 40.) as u32), 1)
    );
    let trace = ask(&mut vcx, handle, "headerDragTrace", &[]);
    let trace: Vec<f32> = trace.split('|').map(|v| v.parse().unwrap()).collect();
    assert_eq!(trace[0] - trace[1], 40.);
    assert_eq!(trace[3], before + 40.);
    let start = edge(&mut vcx, handle, Column::Size);
    vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    let finish = start + point(px(400.), px(0.));
    vcx.simulate_mouse_move(finish, Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_up(finish, MouseButton::Left, Modifiers::default());
    frame(&mut vcx, handle);
    assert_eq!(stored(&mut vcx, handle, Column::Size), (Some(48), 2));
    assert_eq!(ask(&mut vcx, handle, "rowCellOverflow", &["0"]), "0|0|0|0");
    let start = edge(&mut vcx, handle, Column::Size);
    vcx.simulate_click(start, Modifiers::default());
    frame(&mut vcx, handle);
    assert_eq!(stored(&mut vcx, handle, Column::Size), (Some(48), 2));
    handle
        .update(&mut vcx, |shell, _, _| {
            assert_eq!(shell.panes[0].as_ref().unwrap().nav.counters.sorts, 0)
        })
        .unwrap();
}

#[test]
fn u04_double_click_and_f4_fit_only_the_held_rows_without_sorting() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    handle.update(&mut cx, |shell, _, _| {
        shell.launch.settings.data = flea::jsondoc::parse(r#"{"columns":["mode","size","date","kind"],"columnWidths":{"kind":200,"size":200}}"#).unwrap();
        let host = shell.panes[0].as_mut().unwrap();
        let mut rows: Vec<_> = bench::fixture("uniform").into_iter().take(10).collect();
        rows[0].size = 1_500_000_000;
        host.pane.held = Rc::new(Held { listing: 2, total: 405, start: 0, rows });
    }).unwrap();
    frame(&mut cx, handle);
    let at = edge(&mut cx, handle, Column::Kind);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_event(MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        click_count: 2,
        modifiers: Modifiers::default(),
        first_mouse: false,
    });
    vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    frame(&mut vcx, handle);
    assert_eq!(stored(&mut vcx, handle, Column::Kind), (Some(48), 1));
    handle
        .update(&mut vcx, |shell, window, cx| window.focus(&shell.focus, cx))
        .unwrap();
    vcx.simulate_keystrokes("f4");
    frame(&mut vcx, handle);
    let first = stored(&mut vcx, handle, Column::Size).0.unwrap();
    assert!(first < 200);
    let writes = stored(&mut vcx, handle, Column::Size).1;
    vcx.simulate_keystrokes("f4");
    frame(&mut vcx, handle);
    assert_eq!(stored(&mut vcx, handle, Column::Size).1, writes);
    handle
        .update(&mut vcx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let mut rows = bench::fixture("uniform");
            rows.truncate(15);
            rows[0].size = 999_949;
            host.pane.held = Rc::new(Held {
                listing: 2,
                total: 405,
                start: 390,
                rows,
            });
            host.pane.scroll_y = 390. * 31.;
        })
        .unwrap();
    frame(&mut vcx, handle);
    vcx.simulate_keystrokes("f4");
    frame(&mut vcx, handle);
    let later = stored(&mut vcx, handle, Column::Size).0.unwrap();
    assert!(
        later > first,
        "the later held cell did not widen Size: {later} against {first}"
    );
    handle
        .update(&mut vcx, |shell, _, _| {
            assert_eq!(shell.panes[0].as_ref().unwrap().nav.counters.sorts, 0)
        })
        .unwrap();
}

#[test]
fn u04_a_lost_grab_cancels_but_a_busy_listing_keeps_the_drag_and_writes_on_release() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    for busy in [false, true] {
        let at = edge(&mut vcx, handle, Column::Size);
        vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
        let moved = at + point(px(-20.), px(0.));
        vcx.simulate_mouse_move(moved, Some(MouseButton::Left), Modifiers::default());
        if busy {
            handle
                .update(&mut vcx, |shell, _, _| {
                    shell.panes[0].as_mut().unwrap().pane.begin(
                        "/pending".into(),
                        Default::default(),
                        0,
                    );
                })
                .unwrap();
            // A watch refresh re-lists mid-drag; Header.qml:163 keeps the handles and endDrag writes after a move.
            frame(&mut vcx, handle);
            vcx.simulate_mouse_move(
                moved + point(px(-5.), px(0.)),
                Some(MouseButton::Left),
                Modifiers::default(),
            );
        } else {
            vcx.simulate_mouse_move(moved, None, Modifiers::default());
        }
        vcx.simulate_mouse_up(moved, MouseButton::Left, Modifiers::default());
        frame(&mut vcx, handle);
        let (width, writes) = stored(&mut vcx, handle, Column::Size);
        if busy {
            assert!(
                width.is_some() && writes == 1,
                "a busy listing dropped the drag: {width:?} {writes}"
            );
        } else {
            assert_eq!((width, writes), (None, 0), "a lost grab wrote");
        }
    }
}

#[test]
fn u04_search_and_padding_do_not_sort_and_view_changes_cancel_the_grab() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    let start = edge(&mut cx, handle, Column::Size);
    let padding = handle
        .update(&mut cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            point(list.header.drawn.bounds.left() + px(1.), start.y)
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_click(padding, Modifiers::default());
    handle
        .update(&mut vcx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.state.search = true;
            host.selected = vec![1, 2];
        })
        .unwrap();
    frame(&mut vcx, handle);
    vcx.simulate_click(start + point(px(20.), px(0.)), Modifiers::default());
    handle
        .update(&mut vcx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            assert_eq!(host.nav.counters.sorts, 0);
            assert_eq!(host.nav.order, Default::default());
            assert_eq!(host.selected, vec![1, 2]);
            host.pane.state.search = false;
        })
        .unwrap();
    frame(&mut vcx, handle);
    let start = edge(&mut vcx, handle, Column::Size);
    vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    let moved = start + point(px(-20.), px(0.));
    vcx.simulate_mouse_move(moved, Some(MouseButton::Left), Modifiers::default());
    handle
        .update(&mut vcx, |shell, _, cx| {
            shell.view = crate::window::startup::View::Grid;
            cx.notify();
        })
        .unwrap();
    frame(&mut vcx, handle);
    vcx.simulate_mouse_up(moved, MouseButton::Left, Modifiers::default());
    assert_eq!(stored(&mut vcx, handle, Column::Size), (None, 0));
}
