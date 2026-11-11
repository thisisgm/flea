//! Native pointer delivery and the menu's actually painted card bounds.
use super::join_tests::{open, read, ready};
use crate::window::root::Root;
use gpui::{point, px, AppContext, MouseButton, TestAppContext, VisualTestContext, WindowHandle};

fn frame(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> Vec<gpui::Quad> {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window.painted_quads()
    })
    .unwrap()
}

fn key(cx: &mut TestAppContext, handle: WindowHandle<Root>, value: &str) {
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(value);
}

#[gpui::test]
fn u06_live_menu_pointer_mount_paint_hover_disabled_and_outside_press(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut fixture = open(cx);
    let handle = fixture.handle;
    frame(cx, handle);
    handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, _| {
                shell.panes[0].as_mut().unwrap().selected = vec![0, 1];
            })
        })
        .unwrap();
    let point = handle
        .update(cx, |root, window, cx| {
            let shell = root.shell.read(cx);
            let pane = shell.geometry(window).panes[0];
            point(
                px(pane.x + 40.),
                px(pane.y + 27. + 2.5 * crate::list::pane::ROW_HEIGHT),
            )
        })
        .unwrap();
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
        point,
        MouseButton::Right,
        Default::default(),
    );
    ready(cx, &mut fixture);
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "true\n");
    assert_eq!(read(cx, handle, "contextMenuCursor").stdout, "0\n");
    assert_eq!(
        read(cx, handle, "cursor").stdout,
        "2\n",
        "right click captures its own row"
    );
    assert_eq!(
        read(cx, handle, "selectedIndices").stdout,
        "\n",
        "a press outside the marks clears them before capturing its row"
    );
    let rect: Vec<f32> = read(cx, handle, "contextMenuRect")
        .stdout
        .split_whitespace()
        .map(|word| word.parse().unwrap())
        .collect();
    let scale = handle
        .update(cx, |_, window, _| window.scale_factor())
        .unwrap();
    let quads = frame(cx, handle);
    assert!(
        quads.iter().any(|quad| {
            let bounds = quad.bounds;
            (bounds.origin.x.0 / scale - rect[0]).abs() <= 1.
                && (bounds.origin.y.0 / scale - rect[1]).abs() <= 1.
                && (bounds.size.width.0 / scale - rect[2]).abs() <= 1.
                && (bounds.size.height.0 / scale - rect[3]).abs() <= 1.
        }),
        "the reader's card bounds must exist in the painted frame"
    );
    let (rename, paste) = handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let menu = shell.menu.menu.borrow();
            let card = menu.card.as_ref().unwrap();
            let centre = |action| {
                let index = card
                    .entries
                    .iter()
                    .position(|entry| entry.action == action)
                    .unwrap();
                (
                    index,
                    gpui::point(
                        px(card.frame.x + card.frame.w / 2.),
                        px(card.row_screen_top(index)
                            + flea::ui_logic_context_menus::fit::ROW as f32 / 2.),
                    ),
                )
            };
            (centre("rename"), centre("paste"))
        })
        .unwrap();
    key(cx, handle, "escape");
    frame(cx, handle);
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_move(
        rename.1,
        None,
        Default::default(),
    );
    key(cx, handle, "m");
    ready(cx, &mut fixture);
    frame(cx, handle);
    assert_eq!(
        read(cx, handle, "contextMenuCursor").stdout,
        "0\n",
        "a resting pointer does not choose a row"
    );
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_move(
        gpui::point(rename.1.x + px(0.5), rename.1.y),
        None,
        Default::default(),
    );
    let moved = gpui::point(rename.1.x + px(1.), rename.1.y);
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_move(
        moved,
        None,
        Default::default(),
    );
    assert_eq!(
        read(cx, handle, "contextMenuCursor").stdout.trim(),
        rename.0.to_string()
    );
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
        paste.1,
        MouseButton::Left,
        Default::default(),
    );
    assert_eq!(
        read(cx, handle, "contextMenuVisible").stdout,
        "true\n",
        "disabled Paste sends nothing and leaves the menu open"
    );
    assert_eq!(
        read(cx, handle, "contextMenuCursor").stdout.trim(),
        rename.0.to_string()
    );
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
        gpui::point(px(2.), px(2.)),
        MouseButton::Left,
        Default::default(),
    );
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
    assert_eq!(
        read(cx, handle, "cursor").stdout,
        "2\n",
        "the backdrop owns the closing click"
    );
}

#[gpui::test]
fn u06_live_menu_wheel_uses_raw_notches_and_swallowing_keeps_the_pane_still(
    cx: &mut TestAppContext,
) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    key(cx, handle, "m");
    ready(cx, &mut fixture);
    frame(cx, handle);
    let centre = handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let menu = shell.menu.menu.borrow();
            let frame = menu.card.as_ref().unwrap().frame;
            gpui::point(px(frame.x + frame.w / 2.), px(frame.y + frame.h / 2.))
        })
        .unwrap();
    let wheel = |position, delta| gpui::ScrollWheelEvent {
        position,
        delta,
        modifiers: Default::default(),
        touch_phase: gpui::TouchPhase::Moved,
    };
    VisualTestContext::from_window(handle.into(), cx).simulate_event(wheel(
        gpui::point(px(2.), px(2.)),
        gpui::ScrollDelta::Lines(gpui::point(0., -3.)),
    ));
    assert_eq!(read(cx, handle, "contextMenuCursor").stdout, "0\n");
    handle
        .update(cx, |root, _, cx| {
            assert_eq!(
                root.shell.read(cx).panes[0].as_ref().unwrap().pane.scroll_y,
                0.
            )
        })
        .unwrap();
    VisualTestContext::from_window(handle.into(), cx).simulate_event(wheel(
        centre,
        gpui::ScrollDelta::Lines(gpui::point(0., -3.)),
    ));
    assert_eq!(
        read(cx, handle, "contextMenuCursor").stdout,
        "1\n",
        "one native wheel notch moves one menu row"
    );
    VisualTestContext::from_window(handle.into(), cx).simulate_event(wheel(
        centre,
        gpui::ScrollDelta::Pixels(gpui::point(
            px(0.),
            px(-3. * flea::ui_logic_context_menus::fit::ROW as f32),
        )),
    ));
    assert_eq!(
        read(cx, handle, "contextMenuCursor").stdout,
        "3\n",
        "raw row travel skips the separator once"
    );
}
