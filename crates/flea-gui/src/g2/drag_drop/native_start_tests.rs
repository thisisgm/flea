//! B08: a file drag becomes the platform's at the drag threshold, before it leaves the window, and every drop inside its own window still lands through the pane.
use super::mounted_tests::{centre, drag, drain, frame, open, transfer, Root};
use super::router::{self, Native};
use crate::{list::bench, state::Shell, window::startup::View};
use gpui::{
    point, px, ExternalDragPayload, ExternalPaths, FileDropEvent, InputEvent, Modifiers,
    ModifiersChangedEvent, MouseButton, NativeDragAction, NativeDragPayload, Pixels, Point,
    TestAppContext, VisualTestContext, WindowHandle,
};
use std::time::Duration;

// GPUI's drag threshold is 2 px; this move clears it with room to spare.
const PAST_THE_DRAG_THRESHOLD: f32 = 12.;

// Longer than the Columns track slide, so a row's point is read where it comes to rest.
const SETTLE_MS: u64 = 500;

// Presses b.txt in List and moves past the threshold, still over the pane.
fn lift(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> NativeDragPayload {
    handle
        .update(cx, |shell, window, cx| shell.follow_file_drags(window, cx))
        .unwrap();
    frame(cx, handle);
    let file = centre(cx, handle, 1);
    lift_at(cx, handle, file)
}

// The platform accepts the drag at the threshold move, while the pointer is still over the pane.
fn lift_at(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    file: Point<Pixels>,
) -> NativeDragPayload {
    cx.test_window(handle.into())
        .set_start_external_drag_result(true);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.simulate_mouse_move(file, None, Modifiers::default());
    vcx.simulate_mouse_down(file, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        point(file.x + px(PAST_THE_DRAG_THRESHOLD), file.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.run_until_parked();
    let payloads = cx.test_window(handle.into()).external_drag_payloads();
    assert_eq!(
        payloads.len(),
        1,
        "the threshold move starts the platform drag while the pointer is still over the pane"
    );
    let ExternalDragPayload::Mime(payload) = payloads[0].clone() else {
        panic!("the lift's marker and paths reach the platform");
    };
    handle
        .update(cx, |shell, window, cx| {
            assert!(cx.has_active_drag(), "the drag stays live in its window");
            assert!(shell.file_drag_lift(window, cx).is_some());
        })
        .unwrap();
    payload
}

// The platform's own sequence for this window's offer: admission, enter, a move, then the drop.
fn platform_drop(
    vcx: &mut VisualTestContext,
    payload: &NativeDragPayload,
    root: &Root,
    to: Point<Pixels>,
    keys: Modifiers,
    offered: NativeDragAction,
) {
    let types: Vec<String> = payload
        .data
        .representations()
        .iter()
        .map(|entry| entry.mime_type().to_owned())
        .collect();
    let get = |mime: &str| payload.data.get(mime);
    let paths = ExternalPaths(
        [format!("{}/b.txt", root.src()).into()]
            .into_iter()
            .collect(),
    );
    let land = |window: &gpui::Window, cx: &gpui::App, dropped: bool| {
        router::shared(cx).land(&Native::new(
            window.window_handle().window_id().as_u64(),
            (f32::from(to.x), f32::from(to.y)),
            dropped,
            &types,
            &get,
            Some(offered),
            false,
            false,
        ))
    };
    vcx.update(|window, cx| {
        assert!(land(window, cx, false));
        window.dispatch_event(
            FileDropEvent::Entered {
                position: to,
                paths: paths.clone(),
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            gpui::PlatformInput::ModifiersChanged(ModifiersChangedEvent {
                modifiers: keys,
                capslock: Default::default(),
            }),
            cx,
        );
        window.dispatch_event(
            FileDropEvent::Pending { position: to }.to_platform_input(),
            cx,
        );
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        assert!(
            land(window, cx, true),
            "the window accepts its own lift so the platform finishes it"
        );
        window.dispatch_event(
            FileDropEvent::Submit { position: to }.to_platform_input(),
            cx,
        );
    });
    vcx.run_until_parked();
}

fn finish(vcx: &mut VisualTestContext, payload: &NativeDragPayload) {
    vcx.update(|window, cx| {
        router::shared(cx).source(&gpui::NativeDragEvent {
            source_id: payload.source_id,
            action: Some(NativeDragAction::Copy),
            phase: gpui::NativeDragPhase::Finished,
        });
        window.dispatch_event(FileDropEvent::Ended.to_platform_input(), cx);
    });
    vcx.run_until_parked();
}

#[test]
fn b08_a_native_file_drag_lands_on_a_folder_row_of_its_own_window_once() {
    let root = Root::new("b08folder");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    let payload = lift(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    // A router landing would follow the offered Copy; only the pane's own drop moves.
    platform_drop(
        &mut vcx,
        &payload,
        &root,
        folder,
        Modifiers::default(),
        NativeDragAction::Copy,
    );
    let request = transfer(&mut vcx, handle, &mut lines);
    assert!(request.contains(r#""op":"move""#), "{request}");
    assert!(
        request.contains(&format!(r#""dest":"{}/alpha""#, root.src())),
        "{request}"
    );
    finish(&mut vcx, &payload);
    assert!(
        drain(&mut lines)
            .iter()
            .all(|line| !line.contains("transfer") && !line.contains("collisions")),
        "the native copy of the window's own lift is not landed a second time"
    );
    handle
        .update(&mut vcx, |shell, window, cx| {
            assert!(!cx.has_active_drag());
            assert!(shell.file_drag_lift(window, cx).is_none());
        })
        .unwrap();
}

#[test]
fn b08_ctrl_pressed_during_a_native_file_drag_copies_in_its_own_window() {
    let root = Root::new("b08ctrl");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    let payload = lift(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    let ctrl = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    // A router landing would follow the offered Move; only the pane reads Ctrl and copies.
    platform_drop(
        &mut vcx,
        &payload,
        &root,
        folder,
        ctrl,
        NativeDragAction::Move,
    );
    let request = transfer(&mut vcx, handle, &mut lines);
    assert!(request.contains(r#""op":"copy""#), "{request}");
    finish(&mut vcx, &payload);
    assert!(drain(&mut lines)
        .iter()
        .all(|line| !line.contains("transfer") && !line.contains("collisions")));
}

#[test]
fn b08_grid_and_columns_file_drags_start_native_at_the_threshold() {
    for view in [View::Grid, View::Columns] {
        let root = Root::new("b08views");
        let mut cx = bench::real_cx();
        let (handle, _lines) = super::vf3_tests::open(&mut cx, root.src());
        handle
            .update(&mut cx, |shell, window, cx| {
                shell.follow_file_drags(window, cx);
                shell.set_view(view, window, cx);
            })
            .unwrap();
        for _ in 0..4 {
            frame(&mut cx, handle);
            cx.executor()
                .advance_clock(Duration::from_millis(SETTLE_MS));
        }
        handle
            .read_with(&cx, |shell, _| {
                assert_eq!(shell.view, view, "{view:?} is mounted")
            })
            .unwrap();
        let file = super::vf3_spring_cases::hit(&mut cx, handle, 1);
        lift_at(&mut cx, handle, file);
    }
}

#[test]
fn b08_a_refused_native_start_keeps_the_drag_in_its_window() {
    let root = Root::new("b08refused");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            shell.follow_file_drags(window, cx)
        })
        .unwrap();
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let folder = centre(&mut cx, handle, 0);
    cx.test_window(handle.into())
        .set_start_external_drag_result(false);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    drag(&mut vcx, file, folder);
    vcx.run_until_parked();
    assert_eq!(
        cx.test_window(handle.into()).external_drag_payloads().len(),
        1,
        "the threshold move offered the drag once and the platform refused it"
    );
    let request = transfer(&mut vcx, handle, &mut lines);
    assert!(request.contains(r#""op":"move""#), "{request}");
    assert!(
        request.contains(&format!(r#""dest":"{}/alpha""#, root.src())),
        "{request}"
    );
}

#[test]
fn b08_escape_ends_a_native_file_drag_and_moves_nothing() {
    let root = Root::new("b08escape");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    lift(&mut cx, handle);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    handle
        .update(&mut vcx, |shell, window, cx| {
            assert!(shell.escape_file_drag(window, cx));
            assert!(!cx.has_active_drag());
        })
        .unwrap();
    assert_eq!(cx.test_window(handle.into()).cancelled_drags(), 1);
    vcx.run_until_parked();
    assert!(drain(&mut lines)
        .iter()
        .all(|line| !line.contains("transfer") && !line.contains("collisions")));
}

#[test]
fn b08_a_native_file_drag_springs_a_folder_open() {
    let root = Root::new("b08spring");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = super::vf3_tests::open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, _, _| {
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"openFoldersWhileDragging":true}"#).unwrap();
        })
        .unwrap();
    lift(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.update(|window, cx| {
        window.dispatch_event(
            FileDropEvent::Pending { position: folder }.to_platform_input(),
            cx,
        );
    });
    vcx.run_until_parked();
    vcx.executor()
        .advance_clock(Duration::from_millis(super::join::spring::OPEN_MS));
    vcx.run_until_parked();
    let asked = drain(&mut lines);
    assert!(
        asked.iter().any(|line| line.contains(r#""c":"list""#)
            && line.contains(&format!("{}/alpha", root.src()))),
        "the dwell over the folder opens it: {asked:?}"
    );
}
