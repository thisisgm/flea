//! S2 input through the mounted menu layer, including the Locked hero.
use super::join_tests::{open, read, ready};
use crate::{backend::Event, window::startup::View};
use gpui::{point, px, AppContext, ClipboardItem, MouseButton, TestAppContext, VisualTestContext};

#[gpui::test]
fn s2_ey577_right_click_on_locked_hero_opens_its_folder_menu(cx: &mut TestAppContext) {
    for view in [View::List, View::Grid, View::Columns] {
        cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into())));
        let fixture = open(cx);
        let handle = fixture.handle;
        handle
            .update(cx, |root, window, cx| {
                root.shell.update(cx, |shell, cx| {
                    shell.launch.settings.data =
                        flea::jsondoc::parse(r#"{"menu":{"hidden":[]}}"#).unwrap();
                    shell.set_view(view, window, cx);
                });
            })
            .unwrap();
        handle
            .update(cx, |root, window, cx| {
                root.shell.update(cx, |shell, cx| {
                    let host = shell.panes[0].as_mut().unwrap();
                    let now = host.pane.now();
                    assert!(host
                        .pane
                        .begin("/files/Locked".into(), Default::default(), now));
                    assert!(host.pane.busy());
                    shell.receive(
                        0,
                        Event::Refused {
                            operation: "scan".into(),
                            path: "/files/Locked".into(),
                            message: "permission denied".into(),
                            mode: 0o040000,
                        },
                        window,
                        cx,
                    );
                    assert_eq!(
                        shell.panes[0].as_ref().unwrap().pane.state.kind,
                        crate::pane::states::Kind::Locked
                    );
                    if view == View::Columns {
                        crate::window::columns_tests::host::settle(shell, window, cx);
                    }
                    assert_eq!(shell.shown_view(), view);
                    cx.notify();
                });
            })
            .unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        let at = if view == View::Columns {
            let rect = read(cx, handle, "columnActiveRect").stdout;
            let rect: Vec<f32> = rect
                .split_whitespace()
                .map(|word| word.parse().unwrap())
                .collect();
            assert_eq!(rect.len(), 4, "the active track is painted");
            point(px(rect[0] + rect[2] / 2.), px(rect[1] + rect[3] / 2.))
        } else {
            handle
                .update(cx, |root, window, cx| {
                    let pane = root.shell.read(cx).geometry(window).panes[0];
                    point(px(pane.x + pane.width / 2.), px(pane.y + pane.height / 2.))
                })
                .unwrap()
        };
        assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
        VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
            at,
            MouseButton::Right,
            Default::default(),
        );
        assert_eq!(
            read(cx, handle, "contextMenuVisible").stdout,
            "true\n",
            "{view:?}"
        );
        handle
            .update(cx, |root, _, cx| {
                let shell = root.shell.read(cx);
                let menu = shell.menu.menu.borrow();
                assert_eq!(menu.face, super::menu::Face::Locked);
                assert_eq!(shell.locked_folder(0).unwrap().0, "/files/Locked");
            })
            .unwrap();
        let count = handle
            .update(cx, |root, _, cx| {
                root.shell
                    .read(cx)
                    .menu
                    .menu
                    .borrow()
                    .card
                    .as_ref()
                    .unwrap()
                    .entries
                    .len()
            })
            .unwrap();
        let mut selected_copy_path = false;
        for _ in 0..count {
            selected_copy_path = handle
                .update(cx, |root, _, cx| {
                    let shell = root.shell.read(cx);
                    let menu = shell.menu.menu.borrow();
                    let card = menu.card.as_ref().unwrap();
                    card.cursor
                        .is_some_and(|at| card.entries[at].action == "copypath")
                })
                .unwrap();
            if selected_copy_path {
                break;
            }
            VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("down");
        }
        assert!(selected_copy_path, "Copy Path is keyboard reachable");
        VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("enter");
        let copied = cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(copied.as_deref(), Some("/files/Locked"));
    }
}

#[gpui::test]
fn s2_int70_native_notches_step_one_enabled_menu_row(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    ready(cx, &mut fixture);
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    let (at, expected) = handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let mut menu = shell.menu.menu.borrow_mut();
            let card = menu.card.as_mut().unwrap();
            let indices = ["copy", "duplicate", "rename"].map(|action| {
                card.entries
                    .iter()
                    .position(|row| row.action == action)
                    .unwrap()
            });
            card.cursor = Some(indices[0]);
            (
                point(
                    px(card.frame.x + 30.),
                    px(card.row_screen_top(indices[0]) + 10.),
                ),
                [indices[1], indices[2], indices[1]],
            )
        })
        .unwrap();
    for (delta, expected) in [-3., -3., 3.].into_iter().zip(expected) {
        VisualTestContext::from_window(handle.into(), cx).simulate_event(gpui::ScrollWheelEvent {
            position: at,
            delta: gpui::ScrollDelta::Lines(point(0., delta)),
            modifiers: Default::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        assert_eq!(
            read(cx, handle, "contextMenuCursor").stdout,
            format!("{expected}\n")
        );
    }
}
