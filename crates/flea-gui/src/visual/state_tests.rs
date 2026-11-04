use super::*;
use crate::ipc::server::Answer;
use flea::ipc::wire::Request;
use flea::jsondoc::Json;
use gpui::AppContext;

#[gpui::test]
fn native_readbacks_and_each_refusal_use_the_registry(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            // The compositor committed five frames and has shown four; the window drew none with scene work.
            shell.visual.compositor = Some(gpui::PresentedFrames {
                committed: 5,
                presented: 4,
                ..Default::default()
            });
            for (function, arguments, expected) in [
                ("viewMode", vec![], ""),
                ("pendingRequests", vec![], "0"),
                ("previewReadiness", vec![], "absent"),
                ("presentedFrame", vec![], "4"),
                ("listInFlight", vec![], "false"),
            ] {
                let (answer, receive) = std::sync::mpsc::sync_channel(1);
                shell.ipc(
                    Event::Call {
                        request: Request::Call {
                            target: "flea".into(),
                            function: function.into(),
                            arguments,
                        },
                        answer,
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    receive.try_recv().unwrap().reply,
                    flea::ipc::wire::Reply::value(expected)
                );
            }
            for (function, arguments, message) in [
                (
                    "scene",
                    vec!["0".into(), "g1-01-list-r0".into(), "{}".into()],
                    "another window",
                ),
                (
                    "scene",
                    vec![
                        std::process::id().to_string(),
                        "unknown".into(),
                        "{}".into(),
                    ],
                    "not registered",
                ),
                (
                    "nativeGeometry",
                    vec!["0".into(), "g1-01-list-r0".into(), "0".into()],
                    "another window",
                ),
                (
                    "nativeGeometry",
                    vec![
                        std::process::id().to_string(),
                        "g1-01-list-r0".into(),
                        "0".into(),
                    ],
                    "unavailable",
                ),
            ] {
                let (answer, receive) = std::sync::mpsc::sync_channel(1);
                shell.ipc(
                    Event::Call {
                        request: Request::Call {
                            target: "flea".into(),
                            function: function.into(),
                            arguments,
                        },
                        answer,
                    },
                    window,
                    cx,
                );
                let reply = receive.try_recv().unwrap().reply;
                assert_eq!(reply.code, 2);
                assert!(reply.stderr.contains(message), "{reply:?}");
            }
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: "sceneState".into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            );
            let state = flea::jsondoc::parse(&receive.try_recv().unwrap().reply.stdout).unwrap();
            assert_eq!(
                state.get("frame").and_then(flea::jsondoc::Json::as_f64),
                Some(4.)
            );
            assert_eq!(state.get("receipt"), Some(&flea::jsondoc::Json::Null));
        });
    });
}

#[gpui::test]
fn snapshot_that_never_presents_expires_without_a_receipt(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    let (event, receive) = scene_call(
        "g1-01-list-r0",
        r#"{"path":"/files","width":1100,"height":700}"#,
    );
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            forget_drawn(shell);
            shell.ipc(event, window, cx)
        })
    });
    // The compositor numbers each frame's present and reports none of them presented.
    for _ in 0..3 {
        commit_frame(&entity, cx);
    }
    cx.run_until_parked();
    entity.update(cx, |shell, _| {
        assert_eq!(
            (
                drawn_cursor(shell),
                shell.visual.pending.is_some(),
                shell.visual.receipt.is_none()
            ),
            (Some(0), true, true),
            "the window drew the scene, and no frame it drew earned a receipt before the deadline"
        );
    });
    assert!(receive.try_recv().is_err());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1501));
    cx.run_until_parked();
    let reply = receive.try_recv().unwrap().reply;
    assert_eq!(reply.code, 2);
    assert!(reply.stderr.contains("not presented"));
    entity.update(cx, |shell, _| {
        assert!(shell.visual.receipt.is_none());
        assert!(shell.visual.awaiting.is_none() && shell.visual.presented.is_none());
    });
}

fn scene_call(name: &str, settings: &str) -> (Event, std::sync::mpsc::Receiver<Answer>) {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    let request = Request::Call {
        target: "flea".into(),
        function: "scene".into(),
        arguments: vec![std::process::id().to_string(), name.into(), settings.into()],
    };
    (Event::Call { request, answer }, receive)
}

/// Frames the compositor showed before a test's scene, so a presented frame id is never a count the scene drew.
const MAPPED: u64 = 3;

fn shown(frames: u64) -> gpui::PresentedFrames {
    gpui::PresentedFrames {
        committed: frames,
        presented: frames,
        ..Default::default()
    }
}

/// Stands in for the compositor numbering the present of the frame drawn last.
fn commit(shell: &mut Shell) {
    shell.visual.compositor.as_mut().unwrap().committed += 1;
}

/// Stands in for the compositor reporting every committed frame presented, then runs what its observer runs.
fn present(shell: &mut Shell, window: &gpui::Window) {
    let frames = shell.visual.compositor.as_mut().unwrap();
    frames.presented = frames.committed;
    shell.visual_presented(window);
}

/// Forgets the cursor earlier frames drew, so only a frame drawn after this records one.
fn forget_drawn(shell: &Shell) {
    shell.panes[0]
        .as_ref()
        .unwrap()
        .list
        .borrow_mut()
        .drawn_cursor = None;
}

/// The cursor the focused list's last prepaint laid out, the window's own record of a drawn frame.
fn drawn_cursor(shell: &Shell) -> Option<usize> {
    shell.panes[0].as_ref().unwrap().list.borrow().drawn_cursor
}

/// Draws a frame whose present the compositor numbers, and runs that frame's callbacks.
fn commit_frame(entity: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext) -> usize {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        entity.update(cx, |shell, _| commit(shell));
        window.simulate_next_frame(cx)
    })
}

/// Draws a frame and has the compositor present it.
pub(super) fn draw_frame(entity: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext) -> usize {
    let ran = commit_frame(entity, cx);
    cx.update(|window, cx| entity.update(cx, |shell, _| present(shell, window)));
    ran
}

fn readback(
    entity: &gpui::Entity<Shell>,
    cx: &mut gpui::VisualTestContext,
    function: &str,
    arguments: Vec<String>,
) -> flea::ipc::wire::Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    let request = Request::Call {
        target: "flea".into(),
        function: function.into(),
        arguments,
    };
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            shell.ipc(Event::Call { request, answer }, window, cx)
        })
    });
    receive.try_recv().unwrap().reply
}

pub(super) fn root_shell(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<Shell>,
    &mut gpui::VisualTestContext,
    impl FnMut() -> Option<(usize, usize)>,
) {
    let (backend, requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|_, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[0].as_mut().unwrap().answered = true;
        shell.visual.compositor = Some(shown(MAPPED));
        shell
    });
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    (entity, cx, requests)
}

#[gpui::test]
fn presented_scene_receipt_reads_the_root_view_frame(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    let (event, receive) = scene_call(
        "g1-01-list-r0",
        r#"{"path":"/files","width":1100,"height":700}"#,
    );
    cx.update(|window, cx| entity.update(cx, |shell, cx| shell.ipc(event, window, cx)));
    assert!(receive.try_recv().is_err());
    entity.update(cx, |shell, _| assert!(shell.visual.presented.is_some()));
    assert!(commit_frame(&entity, cx) >= 1);
    assert!(
        receive.try_recv().is_err(),
        "a committed frame earns no receipt until the compositor reports it presented"
    );
    cx.update(|window, cx| entity.update(cx, |shell, _| present(shell, window)));
    let reply = receive.try_recv().unwrap().reply;
    assert_eq!(reply.code, 0, "{reply:?}");
    let receipt = flea::jsondoc::parse(&reply.stdout).unwrap();
    let field = |key| receipt.get(key).cloned().unwrap();
    assert_eq!(field("scene"), Json::Str("g1-01-list-r0".into()));
    assert_eq!(field("width").as_f64(), Some(1100.));
    assert_eq!(field("text_size").as_f64(), Some(14.));
    assert_eq!(field("density"), Json::Str("compact".into()));
    assert_eq!(field("reduced_motion"), Json::Bool(true));
    assert_eq!(field("cursor").as_f64(), Some(0.));
    entity.update(cx, |shell, _| {
        assert_eq!(shell.visual.compositor, Some(shown(MAPPED + 1)));
        assert_eq!(field("frame").as_f64(), Some((MAPPED + 1) as f64));
        assert!(shell.visual.awaiting.is_none() && shell.visual.presented.is_none());
        assert!(!shell.visual.probe.empty());
        assert_eq!(shell.visual.receipt.as_ref(), Some(&receipt));
    });
    entity.update(cx, |shell, _| {
        let geometry = shell.visual.probe.json(&receipt);
        let roles = geometry.get("roles").and_then(Json::as_array).unwrap();
        let text = |id: &str| {
            roles
                .iter()
                .find(|role| role.get("id").and_then(Json::as_str) == Some(id))
                .and_then(|role| role.get("text")?.as_str().map(str::to_owned))
        };
        assert_eq!(text("chrome-path").as_deref(), Some("/files"));
        assert!(text("header-0-name").is_some_and(|name| !name.is_empty()));
        assert!(text("status-text").is_some());
    });
    // presentedFrame and nativeGeometry speak the receipt's frame id, the compositor's presented frame.
    let frame = (MAPPED + 1).to_string();
    let pid = std::process::id().to_string();
    let geometry = || vec![pid.clone(), "g1-01-list-r0".into(), frame.clone()];
    assert_eq!(
        readback(&entity, cx, "presentedFrame", vec![]),
        flea::ipc::wire::Reply::value(&frame)
    );
    let reply = readback(&entity, cx, "nativeGeometry", geometry());
    assert_eq!(reply.code, 0, "{reply:?}");
    let sidecar = flea::jsondoc::parse(&reply.stdout).unwrap();
    assert!(sidecar.get("roles").is_some());
    let state = flea::jsondoc::parse(&readback(&entity, cx, "sceneState", vec![]).stdout).unwrap();
    assert_eq!(
        state.get("frame").and_then(Json::as_f64),
        Some((MAPPED + 1) as f64)
    );
    // A newer presented frame makes the receipt's geometry stale.
    draw_frame(&entity, cx);
    let reply = readback(&entity, cx, "nativeGeometry", geometry());
    assert_eq!(reply.code, 2);
    assert!(reply.stderr.contains("unavailable"), "{reply:?}");
    assert_eq!(
        readback(&entity, cx, "presentedFrame", vec![]),
        flea::ipc::wire::Reply::value((MAPPED + 2).to_string())
    );
}

#[gpui::test]
fn a_window_without_presentation_feedback_refuses_the_scene(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    // Without the stand-in the GPUI test window answers, and it reports no presented frames.
    entity.update(cx, |shell, _| shell.visual.compositor = None);
    cx.update(|window, _| assert_eq!(window.presented_frames(), None));
    let (event, receive) = scene_call(
        "g1-01-list-r0",
        r#"{"path":"/files","width":1100,"height":700}"#,
    );
    cx.update(|window, cx| entity.update(cx, |shell, cx| shell.ipc(event, window, cx)));
    let reply = receive.try_recv().unwrap().reply;
    assert_eq!(reply.code, 2);
    assert!(
        reply.stdout.is_empty(),
        "no receipt, zero or otherwise: {reply:?}"
    );
    assert!(reply.stderr.contains("no presented frames"), "{reply:?}");
    let drew = cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.simulate_next_frame(cx)
    });
    assert_eq!(drew, 0, "a refused scene queues no frame work");
    entity.update(cx, |shell, _| {
        assert!(shell.visual.pending.is_none() && shell.visual.receipt.is_none());
    });
    let reply = readback(&entity, cx, "presentedFrame", vec![]);
    assert_eq!(reply.code, 2);
    assert!(reply.stderr.contains("no presented frames"), "{reply:?}");
    let state = flea::jsondoc::parse(&readback(&entity, cx, "sceneState", vec![]).stdout).unwrap();
    assert_eq!(state.get("frame"), Some(&Json::Null));
}

#[gpui::test]
fn a_frame_whose_present_failed_earns_no_receipt(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    let (event, receive) = scene_call(
        "g1-01-list-r0",
        r#"{"path":"/files","width":1100,"height":700}"#,
    );
    cx.update(|window, cx| entity.update(cx, |shell, cx| shell.ipc(event, window, cx)));
    // The present fails, so the compositor numbers no frame and still shows the one before the scene.
    let ran = cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.simulate_next_frame(cx)
    });
    assert!(ran >= 1);
    cx.update(|window, cx| entity.update(cx, |shell, _| present(shell, window)));
    assert!(receive.try_recv().is_err(), "frame {MAPPED} drew no scene");
    draw_frame(&entity, cx);
    let receipt = flea::jsondoc::parse(&receive.try_recv().unwrap().reply.stdout).unwrap();
    assert_eq!(
        receipt.get("frame").and_then(Json::as_f64),
        Some((MAPPED + 1) as f64)
    );
}

#[gpui::test]
fn a_later_frame_shown_in_place_of_the_awaited_one_earns_no_receipt(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    let (event, receive) = scene_call(
        "g1-01-list-r0",
        r#"{"path":"/files","width":1100,"height":700}"#,
    );
    cx.update(|window, cx| entity.update(cx, |shell, cx| shell.ipc(event, window, cx)));
    assert!(commit_frame(&entity, cx) >= 1);
    entity.update(cx, |shell, _| {
        let awaiting = shell.visual.awaiting.as_ref();
        assert_eq!(
            awaiting.map(|awaiting| awaiting.committed),
            Some(MAPPED + 1)
        );
    });
    // The next frame no longer draws the scene's size, and the compositor discards the awaited frame for it.
    cx.simulate_resize(gpui::size(gpui::px(1099.), gpui::px(700.)));
    assert_eq!(commit_frame(&entity, cx), 0);
    cx.update(|window, cx| entity.update(cx, |shell, _| present(shell, window)));
    assert!(
        receive.try_recv().is_err(),
        "frame {} drew another size",
        MAPPED + 2
    );
    expire(&entity, cx, &receive);
}

#[gpui::test]
fn mismatched_client_size_withholds_the_receipt_until_the_deadline(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    let (event, receive) = scene_call(
        "g1-01-list-r0",
        r#"{"path":"/files","width":1099,"height":700}"#,
    );
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            forget_drawn(shell);
            shell.ipc(event, window, cx)
        })
    });
    for _ in 0..3 {
        draw_frame(&entity, cx);
    }
    assert!(receive.try_recv().is_err());
    entity.update(cx, |shell, _| {
        assert_eq!(
            drawn_cursor(shell),
            Some(0),
            "the window drew frames for the pending scene"
        );
        assert!(shell.visual.receipt.is_none());
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1501));
    cx.run_until_parked();
    let reply = receive.try_recv().unwrap().reply;
    assert_eq!(reply.code, 2);
    assert!(reply.stderr.contains("not presented"));
}

#[gpui::test]
fn frames_without_a_scene_queue_no_visual_work(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    for _ in 0..3 {
        assert_eq!(draw_frame(&entity, cx), 0);
    }
    entity.update(cx, |shell, _| {
        assert!(shell.visual.awaiting.is_none() && shell.visual.presented.is_none());
        assert!(shell.visual.probe.empty());
        assert!(shell.panes[0]
            .as_ref()
            .unwrap()
            .list
            .borrow()
            .probe
            .is_none());
    });
}

#[gpui::test]
fn manifest_cursor_and_selection_reach_the_focused_pane(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    let spec = crate::visual::contract::Spec::parse(
        &std::process::id().to_string(),
        "g1-01-list-r0",
        r#"{"path":"/files"}"#,
    )
    .unwrap();
    assert_eq!((spec.cursor, spec.selection.as_slice()), (0, &[][..]));
    let (event, receive) = scene_call("g1-01-list-r0", r#"{"path":"/files"}"#);
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.cursor = 5;
            host.selected = vec![1, 2];
            shell.ipc(event, window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!((host.pane.cursor, host.selected.as_slice()), (0, &[][..]));
        })
    });
    draw_frame(&entity, cx);
    let receipt = flea::jsondoc::parse(&receive.try_recv().unwrap().reply.stdout).unwrap();
    assert_eq!(receipt.get("selection"), Some(&Json::Arr(Vec::new())));
}

fn scene_spec(path: &str, key: &str, value: &str) -> crate::visual::contract::Spec {
    let mut scene = flea::jsondoc::parse(
        r#"{"view":"list","corner_radius":0,"density":"compact","caret_blink":false,"reduced_motion":true,"cursor":0,"selection":[],"text_size":14}"#,
    )
    .unwrap();
    if let Json::Obj(pairs) = &mut scene {
        pairs.retain(|(name, _)| name != key);
        pairs.push((key.into(), flea::jsondoc::parse(value).unwrap()));
    }
    let settings = flea::jsondoc::parse(&format!(r#"{{"path":"{path}"}}"#)).unwrap();
    crate::visual::contract::Spec::from_scene("g1-01-list-r0", &scene, &settings).unwrap()
}

pub(super) fn apply(
    entity: &gpui::Entity<Shell>,
    cx: &mut gpui::VisualTestContext,
    spec: crate::visual::contract::Spec,
) -> std::sync::mpsc::Receiver<Answer> {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| shell.apply_scene(spec, answer, window, cx))
    });
    receive
}

fn land_rows(
    entity: &gpui::Entity<Shell>,
    cx: &mut gpui::VisualTestContext,
    path: &str,
    total: usize,
) {
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let count = shell.panes[0]
                .as_ref()
                .unwrap()
                .pane
                .first_count()
                .min(total);
            for reply in [
                Reply::Listed {
                    path: path.into(),
                    total,
                },
                Reply::Rows {
                    start: 0,
                    rows: vec![flea::protocol::RowFacts::default(); count],
                    listing: 1,
                },
            ] {
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
            }
        })
    });
}

pub(super) fn expire(
    entity: &gpui::Entity<Shell>,
    cx: &mut gpui::VisualTestContext,
    receive: &std::sync::mpsc::Receiver<Answer>,
) {
    for _ in 0..3 {
        draw_frame(entity, cx);
    }
    assert!(
        receive.try_recv().is_err(),
        "no receipt for a frame that cannot draw the scene"
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1501));
    cx.run_until_parked();
    let reply = receive.try_recv().unwrap().reply;
    assert_eq!(reply.code, 2);
    assert!(reply.stderr.contains("not presented"), "{reply:?}");
}

#[gpui::test]
fn a_text_size_the_rows_do_not_draw_earns_no_receipt(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    let receive = apply(&entity, cx, scene_spec("/files", "text_size", "16"));
    expire(&entity, cx, &receive);
    entity.update(cx, |shell, _| assert!(shell.visual.receipt.is_none()));
}

#[gpui::test]
fn the_receipt_text_size_is_the_size_the_rows_drew(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    entity.update(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .list
            .borrow_mut()
            .typeface
            .body = px(16.);
    });
    let receive = apply(&entity, cx, scene_spec("/files", "text_size", "14"));
    expire(&entity, cx, &receive);
    entity.update(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .list
            .borrow_mut()
            .typeface
            .body = px(16.);
    });
    let receive = apply(&entity, cx, scene_spec("/files", "text_size", "16"));
    draw_frame(&entity, cx);
    let receipt = flea::jsondoc::parse(&receive.try_recv().unwrap().reply.stdout).unwrap();
    assert_eq!(receipt.get("text_size").and_then(Json::as_f64), Some(16.));
}

#[gpui::test]
fn a_cursor_after_a_path_change_is_drawn_by_the_receipt_frame(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    let receive = apply(&entity, cx, scene_spec("/other", "cursor", "5"));
    draw_frame(&entity, cx);
    assert!(receive.try_recv().is_err(), "rows have not landed");
    land_rows(&entity, cx, "/other", 40);
    let mut reply = None;
    for _ in 0..4 {
        // Run the callbacks of the frame drawn last, have it presented, then read the cursor that frame's rows drew.
        let (answer, drawn) = cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            entity.update(cx, |shell, _| present(shell, window));
            let drawn = entity.read(cx).panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow()
                .drawn_cursor;
            (receive.try_recv().ok(), drawn)
        });
        if let Some(answer) = answer {
            assert_eq!(
                drawn,
                Some(5),
                "the frame the receipt names drew the scene's cursor"
            );
            reply = Some(answer.reply);
            break;
        }
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            entity.update(cx, |shell, _| commit(shell));
        });
    }
    let reply = reply.expect("a receipt once the rows land");
    assert_eq!(reply.code, 0, "{reply:?}");
    let receipt = flea::jsondoc::parse(&reply.stdout).unwrap();
    assert_eq!(receipt.get("cursor").and_then(Json::as_f64), Some(5.));
    assert_eq!(receipt.get("path"), Some(&Json::Str("/other".into())));
    entity.update(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 5)
    });
}
