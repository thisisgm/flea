//! EY-344 (tests/ui.sh xwdrag): a row lifted in one Flea window and released on another window's floor lands there.
use super::mounted_tests::{centre, frame, open, transfer, Root};
use super::router::{self, Native};
use crate::list::bench;
use gpui::{point, px, Modifiers, MouseButton, VisualTestContext};
use std::sync::Arc;

#[test]
fn ey344_a_row_dragged_from_one_window_lands_on_the_other_windows_floor() {
    let root = Root::new("xwdrag");
    let other = format!("{}/other", root.src().trim_end_matches("/src"));
    std::fs::create_dir_all(&other).unwrap();
    let mut cx = bench::real_cx();
    let (a, _a_lines) = open(&mut cx, root.src());
    let (b, mut b_lines) = open(&mut cx, other.clone());
    for handle in [a, b] {
        handle
            .update(&mut cx, |shell, window, cx| {
                shell.follow_file_drags(window, cx)
            })
            .unwrap();
        frame(&mut cx, handle);
    }
    let file = centre(&mut cx, a, 1);
    let mut avcx = VisualTestContext::from_window(a.into(), &cx);
    avcx.simulate_mouse_move(file, None, Modifiers::default());
    avcx.simulate_mouse_down(file, MouseButton::Left, Modifiers::default());
    // Window A's lift, as the platform carries it out of A.
    let offered = a
        .update(&mut avcx, |shell, window, cx| {
            assert!(shell.begin_file_drag(0, window, cx));
            shell.file_drag_lift(window, cx).unwrap().representations()
        })
        .unwrap();
    let mut bvcx = VisualTestContext::from_window(b.into(), &cx);
    let floor = b
        .update(&mut bvcx, |shell, _, _| {
            let bounds = shell.panes[0].as_ref().unwrap().input.frame.unwrap().bounds;
            point(bounds.origin.x + px(40.), bounds.bottom() - px(12.))
        })
        .unwrap();
    let types: Vec<String> = offered.iter().map(|(mime, _)| (*mime).to_owned()).collect();
    let get = |mime: &str| {
        offered
            .iter()
            .find(|(kind, _)| *kind == mime)
            .map(|(_, bytes)| Arc::<[u8]>::from(bytes.as_slice()))
    };
    b.update(&mut bvcx, |_, window, cx| {
        assert!(router::shared(cx).land(&Native::new(
            window.window_handle().window_id().as_u64(),
            (f32::from(floor.x), f32::from(floor.y)),
            true,
            &types,
            &get,
            None,
            false,
            false
        )));
    })
    .unwrap();
    bvcx.run_until_parked();
    let transfer = transfer(&mut bvcx, b, &mut b_lines);
    assert!(
        transfer.contains(&format!("{}/b.txt", root.src())),
        "{transfer}"
    );
    assert!(
        transfer.contains(&format!(r#""dest":"{other}""#)),
        "{transfer}"
    );
    avcx.simulate_mouse_up(file, MouseButton::Left, Modifiers::default());
}

#[test]
fn sol_xwin_drag_first_native_move_lands_in_a_fresh_empty_list_window() {
    use crate::{backend::Backend, list::pane::Held, pane::states::Kind, state::tests::shell_with};
    use gpui::{size, ExternalDragPayload, ExternalPaths, FileDropEvent, InputEvent};
    use std::rc::Rc;

    let root = Root::new("sol-xwin-drag");
    let other = format!("{}/../empty", root.src());
    std::fs::create_dir_all(&other).unwrap();
    let mut cx = bench::real_cx();
    let (a, mut a_lines) = open(&mut cx, root.src());
    let (backend, mut b_lines) = Backend::test_operation_channel();
    let target = other.clone();
    let b = cx.open_window(size(px(1100.), px(700.)), move |_, cx| {
        let mut shell = shell_with(backend, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.path = target;
        host.pane.held = Rc::new(Held {
            listing: 3,
            total: 0,
            start: 0,
            rows: vec![],
        });
        host.pane.state.kind = Kind::Empty;
        host.list.borrow_mut().typeface = bench::bench_typeface();
        shell
    });
    for handle in [a, b] {
        handle
            .update(&mut cx, |shell, window, cx| {
                shell.follow_file_drags(window, cx)
            })
            .unwrap();
        frame(&mut cx, handle);
    }
    let file = centre(&mut cx, a, 1);
    let mut avcx = VisualTestContext::from_window(a.into(), &cx);
    cx.test_window(a.into())
        .set_start_external_drag_result(true);
    avcx.simulate_mouse_move(file, None, Modifiers::default());
    avcx.simulate_mouse_down(file, MouseButton::Left, Modifiers::default());
    avcx.simulate_mouse_move(
        point(file.x + px(35.), file.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    avcx.simulate_mouse_move(
        point(px(1200.), file.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    avcx.run_until_parked();
    let payloads = cx.test_window(a.into()).external_drag_payloads();
    assert_eq!(payloads.len(), 1, "one real native promotion");
    let ExternalDragPayload::Mime(payload) = &payloads[0] else {
        panic!("file marker and paths must reach the platform");
    };
    let serial = payload.source_id;
    let types: Vec<String> = payload
        .data
        .representations()
        .iter()
        .map(|entry| entry.mime_type().to_owned())
        .collect();
    let get = |mime: &str| payload.data.get(mime);
    let mut bvcx = VisualTestContext::from_window(b.into(), &cx);
    let floor = b
        .update(&mut bvcx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.input.frame.is_none(), "empty paints no row input");
            let bounds = host.list.borrow().drawn.bounds;
            assert!(bounds.size.height > px(100.));
            point(bounds.left() + px(80.), bounds.bottom() - px(20.))
        })
        .unwrap();
    for dropped in [false, true] {
        bvcx.update(|window, cx| {
            assert!(router::shared(cx).land(&Native::new(
                window.window_handle().window_id().as_u64(),
                (f32::from(floor.x), f32::from(floor.y)),
                dropped,
                &types,
                &get,
                Some(gpui::NativeDragAction::Copy),
                false,
                false,
            )));
            let event = if dropped {
                FileDropEvent::Submit { position: floor }
            } else {
                FileDropEvent::Entered {
                    position: floor,
                    paths: ExternalPaths(
                        [format!("{}/b.txt", root.src()).into()]
                            .into_iter()
                            .collect(),
                    ),
                }
            };
            window.dispatch_event(event.to_platform_input(), cx);
        });
        bvcx.run_until_parked();
    }
    let request = transfer(&mut bvcx, b, &mut b_lines);
    assert!(request.contains(r#""op":"move""#), "{request}");
    assert!(
        request.contains(&format!(r#""dest":"{other}""#)),
        "{request}"
    );
    assert!(
        request.contains(&format!("{}/b.txt", root.src())),
        "{request}"
    );
    avcx.update(|window, cx| {
        router::shared(cx).source(&gpui::NativeDragEvent {
            source_id: serial,
            action: Some(gpui::NativeDragAction::Copy),
            phase: gpui::NativeDragPhase::Finished,
        });
        window.dispatch_event(FileDropEvent::Ended.to_platform_input(), cx);
    });
    avcx.run_until_parked();
    a.update(&mut avcx, |shell, window, cx| {
        assert!(shell.file_drag_lift(window, cx).is_none());
    })
    .unwrap();
    assert!(super::mounted_tests::drain(&mut a_lines)
        .iter()
        .all(|line| !line.contains("transfer") && !line.contains("collisions")));
}

#[test]
fn sol_xwin_drag_empty_list_refuses_stale_hidden_and_outside_floors() {
    use crate::{list::pane::Held, pane::states::Kind};
    use flea::ui_logic_drag_drop::URILIST;
    use std::rc::Rc;

    for case in ["stale", "hidden", "outside", "search"] {
        let root = Root::new(case);
        let mut cx = bench::real_cx();
        let (handle, mut lines) = open(&mut cx, root.src());
        handle
            .update(&mut cx, |shell, window, cx| {
                shell.follow_file_drags(window, cx);
                let host = shell.panes[0].as_mut().unwrap();
                host.pane.held = Rc::new(Held {
                    listing: 3,
                    total: 0,
                    start: 0,
                    rows: vec![],
                });
                host.pane.state.kind = Kind::Empty;
                host.input.frame = None;
            })
            .unwrap();
        frame(&mut cx, handle);
        let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
        let floor = handle
            .update(&mut vcx, |shell, window, cx| {
                if case == "search" {
                    assert!(shell.search_action("search", window, cx));
                }
                let host = shell.panes[0].as_mut().unwrap();
                let bounds = host.list.borrow().drawn.bounds;
                let floor = point(bounds.left() + px(80.), bounds.bottom() - px(20.));
                match case {
                    "stale" => host.list.borrow_mut().drawn.listing = 2,
                    "hidden" => host.drawn_view = None,
                    "outside" => return point(floor.x, bounds.top() - px(1.)),
                    _ => {}
                }
                floor
            })
            .unwrap();
        super::mounted_tests::drain(&mut lines);
        let uris = flea::ui_logic_file_clipboard::build_urilist(&[format!("{}/b.txt", root.src())]);
        let types = vec![URILIST.to_owned()];
        let get = |mime: &str| (mime == URILIST).then(|| Arc::<[u8]>::from(uris.as_slice()));
        vcx.update(|window, cx| {
            assert!(router::shared(cx).land(&Native::new(
                window.window_handle().window_id().as_u64(),
                (f32::from(floor.x), f32::from(floor.y)),
                true,
                &types,
                &get,
                None,
                false,
                false,
            )));
        });
        vcx.run_until_parked();
        assert!(
            super::mounted_tests::drain(&mut lines)
                .iter()
                .all(|line| !line.contains("collisions") && !line.contains("transfer")),
            "{case} floor must never send a file operation"
        );
    }
}
