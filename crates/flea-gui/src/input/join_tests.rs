use super::*;
use flea::ipc::wire::{Reply as IpcReply, Request};
use gpui::{point, KeyDownEvent, Keystroke, Modifiers, MouseButton};

fn call(
    shell: &mut Shell,
    name: &str,
    window: &mut gpui::Window,
    cx: &mut Context<Shell>,
) -> IpcReply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    shell.ipc(
        Event::Call {
            request: Request::Call {
                target: "flea".into(),
                function: name.into(),
                arguments: vec![],
            },
            answer,
        },
        window,
        cx,
    );
    receive.try_recv().expect("live IPC answered").reply
}

fn key(shell: &mut Shell, name: &str, window: &mut gpui::Window, cx: &mut Context<Shell>) {
    shell.key(
        &KeyDownEvent {
            keystroke: {
                let mut stroke = Keystroke::parse(name).unwrap();
                if stroke.key.chars().count() == 1 {
                    stroke.key_char = Some(if stroke.modifiers.shift {
                        stroke.key.to_uppercase()
                    } else {
                        stroke.key.clone()
                    });
                }
                stroke
            },
            is_held: false,
            prefer_character_input: false,
        },
        window,
        cx,
    );
}

#[gpui::test]
fn u03_readers_observe_live_focus_and_refuse_undrawn_geometry(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let window = cx.add_empty_window();
    let shell = window.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    window.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            window.focus(&shell.focus, cx);
            assert_eq!(
                call(shell, "focusView", window, cx),
                IpcReply::value("list")
            );
            assert_ne!(
                call(shell, "scrollbarState", window, cx).code,
                0,
                "no frame was drawn"
            );
            assert_eq!(call(shell, "selectionBandState", window, cx).code, 0);
        })
    });
}

#[gpui::test]
fn u03_selection_keys_extend_shrink_and_listing_gate(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let window = cx.add_empty_window();
    let shell = window.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    window.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            window.focus(&shell.focus, cx);
            key(shell, "v", window, cx);
            assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0]);
            key(shell, "shift-down", window, cx);
            key(shell, "shift-down", window, cx);
            assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0, 1, 2]);
            key(shell, "shift-up", window, cx);
            assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0, 1]);
            key(shell, "escape", window, cx);
            assert!(shell.panes[0].as_ref().unwrap().selected.is_empty());
            shell.panes[0]
                .as_mut()
                .unwrap()
                .pane
                .begin("/next".into(), Default::default(), 0);
            key(shell, "ctrl-a", window, cx);
            assert!(shell.panes[0].as_ref().unwrap().selected.is_empty());
        })
    });
}

#[gpui::test]
fn u03_native_pointer_release_scrollbar_and_modal_failure_paths(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_resize(size(px(900.), px(600.)));
    cx.run_until_parked();
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .input
            .frame
            .expect("real prepaint")
    });
    let row = |n: f32| {
        point(
            frame.bounds.left() + px(60.),
            frame.bounds.top() + px(frame.row * (n + 0.5)),
        )
    };
    cx.simulate_mouse_move(row(2.), None, Modifiers::none());
    cx.simulate_mouse_down(row(2.), MouseButton::Left, Modifiers::none());
    shell.read_with(cx, |shell, _| {
        assert!(
            shell.panes[0].as_ref().unwrap().selected.is_empty(),
            "press preserves marks until release"
        )
    });
    cx.simulate_mouse_up(row(2.), MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [2])
    });
    cx.simulate_mouse_down(
        row(4.),
        MouseButton::Left,
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    cx.simulate_mouse_up(
        row(4.),
        MouseButton::Left,
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [2, 4])
    });
    cx.simulate_mouse_down(row(4.), MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(row(4.), MouseButton::Right, Modifiers::none());
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [2, 4])
    });
    cx.simulate_mouse_move(row(6.), None, Modifiers::none());
    cx.simulate_mouse_down(row(6.), MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(row(6.), MouseButton::Right, Modifiers::none());
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.selected.is_empty());
        assert_eq!(host.pane.cursor, 6);
    });
    let lane = point(
        frame.bounds.right() - px(3.),
        frame.bounds.top() + frame.bounds.size.height / 2.,
    );
    cx.simulate_mouse_move(lane, None, Modifiers::none());
    cx.simulate_mouse_down(lane, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.pane.scroll_y > frame.viewport() * 2.,
            "track jumps before release"
        );
        assert!(host.input.drawn.unwrap().shown);
        assert!(host.input.drawn.unwrap().knob.size.width > px(6.));
    });
    cx.simulate_mouse_up(lane, MouseButton::Left, Modifiers::none());
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let state = call(shell, "scrollbarState", window, cx);
            assert_eq!(state.code, 0);
            assert!(flea::jsondoc::parse(&state.stdout)
                .unwrap()
                .get("visible")
                .unwrap()
                .as_bool()
                .unwrap());
            shell.input.blocked = true;
        })
    });
    let before = shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor);
    cx.simulate_mouse_down(row(1.), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(row(1.), MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, before)
    });
}

#[gpui::test]
fn u03_upward_band_restores_marks_on_cancel_and_keeps_scroll_on_release(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.held = Rc::new(crate::list::pane::Held {
            listing: 3,
            total: 4,
            start: 0,
            rows: vec![Default::default(); 4],
        });
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_resize(size(px(900.5), px(600.)));
    cx.run_until_parked();
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let start = point(
        frame.bounds.left() + px(100.),
        frame.bounds.top() + px(frame.row * 4. + 10.),
    );
    let end = point(
        frame.bounds.left() + px(50.),
        frame.bounds.top() + px(frame.row / 2.),
    );
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.selected, [0, 1, 2, 3]);
        assert!(host.input.band.as_ref().unwrap().active);
    });
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 0);
        assert_eq!(host.pane.scroll_y, 0.);
        assert!(host.input.band.is_none());
    });
    let shrink = point(end.x, frame.bounds.top() + px(frame.row * 2.5));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(shrink, Some(MouseButton::Left), Modifiers::none());
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [2, 3])
    });
    cx.update(|window, cx| shell.update(cx, |shell, cx| key(shell, "escape", window, cx)));
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0, 1, 2, 3])
    });
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(shrink, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_resize(size(px(850.), px(600.)));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.input.band.is_none());
        assert_eq!(host.selected, [0, 1, 2, 3]);
        let list = host.list.borrow();
        assert!(list
            .visible()
            .iter()
            .filter(|row| row.index < 4)
            .all(|row| row.state.selected));
    });
}

#[gpui::test]
fn u03_real_card_captures_keys_and_navigation_marks_reach_the_painter(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.panes[0].as_mut().unwrap().selected = vec![2, 4];
            shell.after_navigation(0, "/files", window, cx);
            shell.open_overlay_copy_to(false, "/files", window, cx);
            key(shell, "down", window, cx);
            key(shell, "v", window, cx);
            key(shell, "ctrl-a", window, cx);
            assert_eq!(shell.panes[0].as_ref().unwrap().selected, [2, 4]);
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 0);
            shell.close_overlay(window, cx);
        })
    });
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let list = host.list.borrow();
        assert!(
            list.visible()
                .iter()
                .find(|row| row.index == 2)
                .unwrap()
                .state
                .selected
        );
        assert!(
            !list
                .visible()
                .iter()
                .find(|row| row.index == 3)
                .unwrap()
                .state
                .selected
        );
    });
}

#[path = "policy_tests.rs"]
mod policy_tests;

#[path = "geometry_tests.rs"]
mod geometry_tests;

#[path = "fix1_tests.rs"]
mod fix1_tests;

#[path = "closing_tests.rs"]
mod closing_tests;

#[path = "fix2_tests.rs"]
mod fix2_tests;

#[path = "g1_05_tests.rs"]
mod g1_05_tests;

#[path = "pagekeys_tests.rs"]
mod pagekeys_tests;

#[path = "fixlist_1101_tests.rs"]
mod fixlist_1101_tests;
