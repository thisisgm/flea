use super::*;
use crate::list::columns::Column;
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext};

fn title(cx: &mut TestAppContext, handle: Handle, side: usize) -> gpui::Point<gpui::Pixels> {
    edit(cx, handle, |shell, _, _| {
        let list = shell.panes[side].as_ref().unwrap().list.borrow();
        let bounds = list.header.drawn.bounds;
        assert!(
            f32::from(bounds.size.height) > 0.,
            "the real header is absent"
        );
        let cell = list.header.drawn.layout.cell(Column::Size).unwrap();
        point(
            bounds.left() + px(cell.x + cell.width / 2.),
            bounds.center().y,
        )
    })
}

#[gpui::test]
fn gcomp_dual_and_list_mount_headers_and_cancel_the_hidden_right_press(cx: &mut TestAppContext) {
    let handle = open(cx);
    for side in 0..2 {
        let _ = title(cx, handle, side);
    }
    let at = title(cx, handle, 1);
    let before = edit(cx, handle, |shell, _, _| {
        shell.panes[1].as_ref().unwrap().nav.order
    });
    {
        let mut pointer = VisualTestContext::from_window(handle.into(), cx);
        pointer.simulate_mouse_move(at, None, Modifiers::default());
        pointer.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    }
    edit(cx, handle, |shell, window, cx| {
        assert!(shell.panes[1]
            .as_ref()
            .unwrap()
            .list
            .borrow()
            .header
            .press
            .is_some());
        shell.set_view(View::List, window, cx);
    });
    frame(cx, handle);
    let _ = title(cx, handle, 0);
    edit(cx, handle, |shell, window, cx| {
        assert!(shell.panes[1]
            .as_ref()
            .unwrap()
            .list
            .borrow()
            .header
            .press
            .is_none());
        shell.set_view(View::Dual, window, cx);
    });
    frame(cx, handle);
    {
        let mut pointer = VisualTestContext::from_window(handle.into(), cx);
        pointer.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    }
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.panes[1].as_ref().unwrap().nav.order, before)
    });
    let at = title(cx, handle, 1);
    {
        let mut pointer = VisualTestContext::from_window(handle.into(), cx);
        pointer.simulate_mouse_move(at, None, Modifiers::default());
        pointer.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
        pointer.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    }
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.panes[1].as_ref().unwrap().nav.order.key, "size");
        assert_eq!(shell.panes[0].as_ref().unwrap().nav.order.key, "name");
    });
}

#[gpui::test]
fn gcomp_every_chrome_mouse_button_preserves_both_panes_keyboard_owner(cx: &mut TestAppContext) {
    let handle = open(cx);
    let at = point(px(300.), px(10.));
    for side in 0..2 {
        for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
            let before = edit(cx, handle, |shell, window, cx| {
                shell.focus_pane(side, cx);
                window.focus(&shell.focus, cx);
                std::array::from_fn::<_, 2, _>(|at| shell.panes[at].as_ref().unwrap().pane.cursor)
            });
            frame(cx, handle);
            {
                let mut pointer = VisualTestContext::from_window(handle.into(), cx);
                pointer.simulate_mouse_move(at, None, Modifiers::default());
                pointer.simulate_mouse_down(at, button, Modifiers::default());
                pointer.simulate_mouse_up(at, button, Modifiers::default());
                pointer.simulate_keystrokes("down");
            }
            edit(cx, handle, |shell, window, _| {
                assert_eq!(shell.focus_side, side, "{button:?} changed pane ownership");
                assert!(
                    shell.focus.is_focused(window),
                    "{button:?} stole listing focus"
                );
                for (at, cursor) in before.into_iter().enumerate() {
                    assert_eq!(
                        shell.panes[at].as_ref().unwrap().pane.cursor,
                        cursor + usize::from(at == side),
                        "{button:?} sent Down to the wrong pane"
                    );
                }
            });
        }
    }
    assert_eq!(
        call(cx, handle, "railState").code,
        3,
        "queued U08 must retain its refusal"
    );
    assert_eq!(
        call(cx, handle, "thumbnailPolicyState").code,
        0,
        "thumbnailPolicyState answers its v0.3.12 shape while the pane is idle"
    );
}

#[path = "gcomp_input_repair_tests.rs"]
mod repair;
