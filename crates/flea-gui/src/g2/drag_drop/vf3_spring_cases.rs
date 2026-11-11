//! Every mounted view uses the same dwell and cancellation policy, with real native enter/leave delivery.
use super::mounted_tests::{frame, Root};
use super::vf3_tests::open;
use crate::{list::bench, state::Shell, window::startup::View};
use gpui::{
    point, px, ExternalPaths, FileDropEvent, InputEvent, Modifiers, MouseButton, Pixels, Point,
    TestAppContext, VisualTestContext, WindowHandle,
};
use std::{path::PathBuf, time::Duration};

pub(super) fn hit(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    row: usize,
) -> Point<Pixels> {
    handle
        .read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            if shell.view == View::Columns {
                let bounds = host.input.frame.unwrap().bounds;
                for y in 0..f32::from(bounds.size.height) as usize {
                    for x in (0..f32::from(bounds.size.width) as usize).step_by(20) {
                        let at = point(bounds.left() + px(x as f32), bounds.top() + px(y as f32));
                        if crate::window::columns::hover_at(host, at).is_some_and(|hover| {
                            hover.slot == crate::columns::geometry::Slot::Active
                                && hover.index == row
                        }) {
                            return at + point(px(20.), px(8.));
                        }
                    }
                }
            }
            if let Some(boxes) = &host.input.hit_boxes {
                let (_, bounds) = boxes.iter().find(|(index, _)| *index == row).unwrap();
                return bounds.center();
            }
            let bounds = host.input.frame.unwrap().bounds;
            for y in 0..f32::from(bounds.size.height) as usize {
                let at = point(bounds.left() + px(60.), bounds.top() + px(y as f32));
                if host.input.hit(at) == Some(row) {
                    return at + point(px(0.), px(8.));
                }
            }
            panic!("row {row} has no painted hit in {:?}", host.drawn_view);
        })
        .unwrap()
}

fn run(view: View, native: bool, action: &str) {
    let root = Root::new("dwell-case");
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, root.src());
    let (right_backend, _right_lines) = crate::backend::Backend::test_lines();
    handle
        .update(&mut cx, |shell, window, cx| {
            if view == View::Dual {
                let rows = shell.panes[0].as_ref().unwrap().pane.held.rows.clone();
                let mut right =
                    crate::window::pane::PaneHost::new(root.src(), None, 646., &shell.locale);
                right.backend = Some(right_backend);
                right.pane.use_executor(cx.background_executor().clone());
                right
                    .pane
                    .receive(flea::protocol::Reply::Listed {
                        path: root.src(),
                        total: rows.len(),
                    })
                    .unwrap();
                right
                    .pane
                    .receive(flea::protocol::Reply::Rows {
                        start: 0,
                        listing: 2,
                        rows,
                    })
                    .unwrap();
                right.changed(&shell.locale);
                right
                    .nav
                    .after_reply(&mut right.pane, &root.src(), &mut right.selected);
                shell.panes[1] = Some(right);
            }
            shell.set_view(view, window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.cursor = 1;
            host.selected = vec![1];
            shell.launch.settings.data = flea::jsondoc::parse(if action == "default" {
                "{}"
            } else if action == "off" {
                r#"{"openFoldersWhileDragging":false}"#
            } else {
                r#"{"openFoldersWhileDragging":true}"#
            })
            .unwrap();
        })
        .unwrap();
    // Columns slides its tracks into place, so points are read only after the motion settles.
    for _ in 0..4 {
        frame(&mut cx, handle);
        cx.executor().advance_clock(Duration::from_millis(500));
    }
    handle
        .read_with(&cx, |shell, _| {
            assert_eq!(shell.view, view, "the actual view must be mounted")
        })
        .unwrap();
    let from = hit(&mut cx, handle, 1);
    let folder = hit(&mut cx, handle, 0);
    let mut cx = VisualTestContext::from_window(handle.into(), &cx);
    if native {
        cx.update(|window, cx| {
            window.dispatch_event(
                FileDropEvent::Entered {
                    position: folder,
                    paths: ExternalPaths(
                        [PathBuf::from(format!("{}/b.txt", root.src()))]
                            .into_iter()
                            .collect(),
                    ),
                }
                .to_platform_input(),
                cx,
            );
        });
    } else {
        cx.simulate_mouse_move(from, None, Modifiers::default());
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(
            from + point(px(20.), px(0.)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        cx.simulate_mouse_move(folder, Some(MouseButton::Left), Modifiers::default());
    }
    cx.run_until_parked();
    handle
        .update(&mut cx, |shell, window, cx| {
            assert!(
                cx.has_active_drag(),
                "view {view:?}, native {native} must hold an actual drag"
            );
            let host = shell.panes[0].as_ref().unwrap();
            let at = window.mouse_position();
            assert_eq!(
                if view == View::Columns {
                    crate::window::columns::drop_spot(host, at, &[])
                } else {
                    Some(super::join::pane_spot(host, at, &[]))
                },
                Some(format!("{}/alpha", root.src())),
                "view {view:?}, native {native} must hover the painted folder"
            );
        })
        .unwrap();
    if matches!(action, "move" | "stationary") {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
        cx.simulate_mouse_move(
            folder + point(px(if action == "move" { 1. } else { 0. }), px(0.)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
    } else if action == "leave" {
        cx.update(|window, cx| {
            window.dispatch_event(FileDropEvent::Exited.to_platform_input(), cx);
        });
    } else if action == "escape" {
        cx.simulate_keystrokes("escape");
    }
    cx.executor()
        .advance_clock(Duration::from_millis(if action == "stationary" {
            100
        } else {
            300
        }));
    cx.run_until_parked();
    let expected = if matches!(action, "open" | "move" | "stationary") {
        format!("{}/alpha", root.src())
    } else {
        root.src()
    };
    handle
        .read_with(&cx, |shell, _| {
            assert_eq!(
                {
                    let host = shell.panes[0].as_ref().unwrap();
                    host.nav
                        .asked(&host.pane)
                        .unwrap_or_else(|| host.path.clone())
                },
                expected,
                "view {view:?}, native {native}, action {action}"
            )
        })
        .unwrap();
    cx.simulate_keystrokes("escape");
}

#[test]
fn vf3_spring_all_views_and_native_enter() {
    for view in [View::List, View::Columns, View::Grid, View::Dual] {
        for native in [false, true] {
            run(view, native, "open");
        }
    }
}

#[test]
fn vf3_spring_honors_default_off_escape_movement_and_leave() {
    for action in ["default", "off", "escape", "move", "stationary", "leave"] {
        run(View::List, action == "leave", action);
    }
}
