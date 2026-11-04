//! Header.qml's TapHandler sorts on release, and autofitAll writes one patch.
use super::{frame, open};
use crate::{
    list::{bench, columns::Column},
    state::Shell,
    window::startup::View,
};
use gpui::{point, px, Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle};

fn order(cx: &mut VisualTestContext, handle: gpui::WindowHandle<crate::state::Shell>) -> String {
    handle
        .update(cx, |shell, _, _| {
            format!("{:?}", shell.panes[0].as_ref().unwrap().nav.order)
        })
        .unwrap()
}

#[test]
fn u04_a_title_sorts_on_release_and_a_press_dragged_off_it_never_does() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    let title = handle
        .update(&mut cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            let header = list.header.drawn.bounds;
            let cell = list.header.drawn.layout.cell(Column::Size).unwrap();
            point(
                header.left() + px(cell.x + cell.width / 2.),
                header.center().y,
            )
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    let untouched = order(&mut vcx, handle);
    vcx.simulate_mouse_move(title, None, Modifiers::default());
    vcx.simulate_mouse_down(title, MouseButton::Left, Modifiers::default());
    assert_eq!(order(&mut vcx, handle), untouched, "the press alone sorted");
    let away = title + point(px(0.), px(200.));
    vcx.simulate_mouse_move(away, Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_up(away, MouseButton::Left, Modifiers::default());
    frame(&mut vcx, handle);
    assert_eq!(
        order(&mut vcx, handle),
        untouched,
        "a press dragged off the title sorted"
    );
    // Past the slop and back is still a drag, as a TapHandler cancels once the press travels.
    vcx.simulate_mouse_down(title, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        title + point(px(40.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.simulate_mouse_up(title, MouseButton::Left, Modifiers::default());
    assert_eq!(
        order(&mut vcx, handle),
        untouched,
        "a drag that came back sorted"
    );
    vcx.simulate_mouse_down(title, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        title + point(px(3.), px(1.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.simulate_mouse_up(
        title + point(px(3.), px(1.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    frame(&mut vcx, handle);
    assert!(
        order(&mut vcx, handle).contains("\"size\""),
        "a tap did not sort on release"
    );
}

#[test]
fn u04_one_f4_persists_its_fitted_columns_in_one_write() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    handle
        .update(&mut cx, |shell, _, _| {
            shell.launch.settings.data = flea::jsondoc::parse(
                r#"{"columns":["mode","size","date","kind"],"columnWidths":{"mode":300,"size":300,"date":300,"kind":300}}"#,
            )
            .unwrap();
        })
        .unwrap();
    frame(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    handle
        .update(&mut vcx, |shell, window, cx| window.focus(&shell.focus, cx))
        .unwrap();
    vcx.simulate_keystrokes("f4");
    frame(&mut vcx, handle);
    let (sent, stored) = handle
        .update(&mut vcx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let list = host.list.borrow();
            (
                host.nav.writer.borrow().sent,
                Column::ALL.map(|column| list.view.stored.get(column)),
            )
        })
        .unwrap();
    assert_eq!(sent, 1, "one F4 queued {sent} separate ui.json writes");
    assert!(
        stored
            .iter()
            .all(|width| width.is_some_and(|width| width < 300)),
        "{stored:?}"
    );
}

fn title(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> gpui::Point<gpui::Pixels> {
    handle
        .update(cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            let header = list.header.drawn.bounds;
            let cell = list.header.drawn.layout.cell(Column::Size).unwrap();
            point(
                header.left() + px(cell.x + cell.width / 2.),
                header.center().y,
            )
        })
        .unwrap()
}

fn sorts(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> usize {
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().nav.counters.sorts
        })
        .unwrap()
}

#[test]
fn u04_fix2_a_hidden_header_cannot_complete_its_old_tap() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    let at = title(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(at, None, Modifiers::default());
    vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    assert_eq!(sorts(&mut vcx, handle), 0);
    handle
        .update(&mut vcx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(&mut vcx, handle);
    vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    let count = sorts(&mut vcx, handle);
    eprintln!("REVIEW-R2 hidden-header sorts={count}");
    assert_eq!(
        count, 0,
        "a header absent from the view completed its old tap"
    );
}

#[test]
fn u04_fix2_a_title_tap_losing_its_left_grab_cannot_sort() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    let at = title(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(at, None, Modifiers::default());
    vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(at + point(px(1.), px(0.)), None, Modifiers::default());
    vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    let count = sorts(&mut vcx, handle);
    eprintln!("REVIEW-R2 lost-title-grab sorts={count}");
    assert_eq!(count, 0, "a cancelled title press later sorted");
}

mod fix3 {
    //! Focused cancellation checks through the real window's header event listeners.
    use super::{frame, open};
    use crate::{
        backend::Backend,
        list::{bench, columns::Column},
        state::Shell,
        window::startup::View,
    };
    use gpui::{
        point, px, Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle,
    };

    fn title(
        cx: &mut TestAppContext,
        handle: WindowHandle<Shell>,
        side: usize,
    ) -> gpui::Point<gpui::Pixels> {
        handle
            .update(cx, |shell, _, _| {
                let list = shell.panes[side].as_ref().unwrap().list.borrow();
                let bounds = list.header.drawn.bounds;
                let cell = list.header.drawn.layout.cell(Column::Size).unwrap();
                point(
                    bounds.left() + px(cell.x + cell.width / 2.),
                    bounds.center().y,
                )
            })
            .unwrap()
    }

    fn sorts(cx: &mut TestAppContext, handle: WindowHandle<Shell>, side: usize) -> usize {
        handle
            .update(cx, |shell, _, _| {
                shell.panes[side].as_ref().unwrap().nav.counters.sorts
            })
            .unwrap()
    }

    #[test]
    fn u04_fix3_hiding_the_right_dual_header_cancels_its_pending_tap() {
        let mut cx = bench::real_cx();
        let handle = open(&mut cx);
        handle
            .update(&mut cx, |shell, _, cx| {
                let (spare, _) = Backend::test_channel();
                shell.panes[1] = crate::state::tests::shell_with(spare, cx).panes[0].take();
                let host = shell.panes[1].as_mut().unwrap();
                host.backend = None;
                host.list.borrow_mut().typeface = bench::bench_typeface();
                shell.view = View::Dual;
            })
            .unwrap();
        frame(&mut cx, handle);
        let at = title(&mut cx, handle, 1);
        let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
        vcx.simulate_mouse_move(at, None, Modifiers::default());
        vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
        assert_eq!(sorts(&mut vcx, handle, 1), 0, "press alone sorted");
        handle
            .update(&mut vcx, |shell, window, cx| {
                shell.set_view(View::List, window, cx)
            })
            .unwrap();
        frame(&mut vcx, handle);
        let pending = handle
            .update(&mut vcx, |shell, _, _| {
                shell.panes[1]
                    .as_ref()
                    .unwrap()
                    .list
                    .borrow()
                    .header
                    .press
                    .is_some()
            })
            .unwrap();
        vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
        let count = sorts(&mut vcx, handle, 1);
        eprintln!("REVIEW-R3 hidden-right-header pending={pending} sorts={count}");
        assert_eq!(
            count, 0,
            "the hidden right Dual header completed its old tap"
        );
    }

    #[test]
    fn u04_fix3_release_outside_list_dual_is_refused_before_a_repaint() {
        let mut cx = bench::real_cx();
        let handle = open(&mut cx);
        frame(&mut cx, handle);
        let at = title(&mut cx, handle, 0);
        let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
        vcx.simulate_mouse_move(at, None, Modifiers::default());
        vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
        handle
            .update(&mut vcx, |shell, window, cx| {
                shell.set_view(View::Columns, window, cx);
                shell.list_header_up(
                    &gpui::MouseUpEvent {
                        button: MouseButton::Left,
                        position: at,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                    },
                    window,
                    cx,
                );
            })
            .unwrap();
        let count = sorts(&mut vcx, handle, 0);
        eprintln!("REVIEW-R3 pre-repaint non-list release sorts={count}");
        assert_eq!(count, 0, "a non-List/Dual release sorted before repaint");
    }

    #[test]
    fn u04_fix3_a_visible_right_dual_header_still_sorts_on_release() {
        let mut cx = bench::real_cx();
        let handle = open(&mut cx);
        handle
            .update(&mut cx, |shell, _, cx| {
                let (spare, _) = Backend::test_channel();
                shell.panes[1] = crate::state::tests::shell_with(spare, cx).panes[0].take();
                let host = shell.panes[1].as_mut().unwrap();
                host.backend = None;
                host.list.borrow_mut().typeface = bench::bench_typeface();
                shell.view = View::Dual;
            })
            .unwrap();
        frame(&mut cx, handle);
        let at = title(&mut cx, handle, 1);
        let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
        vcx.simulate_mouse_move(at, None, Modifiers::default());
        vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
        assert_eq!(sorts(&mut vcx, handle, 1), 0, "press alone sorted");
        vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
        assert_eq!(sorts(&mut vcx, handle, 0), 0, "the left pane sorted");
        assert_eq!(
            sorts(&mut vcx, handle, 1),
            1,
            "visible right tap did not sort"
        );
    }
}
