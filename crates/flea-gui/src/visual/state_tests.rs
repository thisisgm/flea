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
            for (function, arguments, expected) in [
                ("viewMode", vec![], "list"),
                ("pendingRequests", vec![], "0"),
                ("previewReadiness", vec![], "absent"),
                ("presentedFrame", vec![], "0"),
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
                Some(0.)
            );
            assert_eq!(state.get("receipt"), Some(&flea::jsondoc::Json::Null));
        });
    });
}

#[gpui::test]
fn snapshot_that_never_presents_expires_without_a_receipt(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let window = cx.add_empty_window();
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    let entity = window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: "scene".into(),
                        arguments: vec![
                            std::process::id().to_string(),
                            "g1-01-list-r0".into(),
                            r#"{"path":"/files","width":1099}"#.into(),
                        ],
                    },
                    answer,
                },
                window,
                cx,
            );
            assert!(shell.visual.pending.is_some());
        });
        entity
    });
    cx.run_until_parked();
    assert!(receive.try_recv().is_err());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1501));
    cx.run_until_parked();
    let reply = receive.try_recv().unwrap().reply;
    assert_eq!(reply.code, 2);
    assert!(reply.stderr.contains("not presented"));
    entity.update(cx, |shell, _| {
        assert!(shell.visual.receipt.is_none());
        assert_eq!(shell.visual.frame, 0);
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

fn draw_frame(cx: &mut gpui::VisualTestContext) -> usize {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.simulate_next_frame(cx)
    })
}

fn root_shell(
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
    assert!(draw_frame(cx) >= 1);
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
        let presented = field("frame").as_f64().unwrap();
        assert!(presented >= 1. && presented <= shell.visual.frame as f64);
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
}

#[gpui::test]
fn mismatched_client_size_withholds_the_receipt_until_the_deadline(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    let (event, receive) = scene_call(
        "g1-01-list-r0",
        r#"{"path":"/files","width":1099,"height":700}"#,
    );
    cx.update(|window, cx| entity.update(cx, |shell, cx| shell.ipc(event, window, cx)));
    for _ in 0..3 {
        draw_frame(cx);
    }
    assert!(receive.try_recv().is_err());
    entity.update(cx, |shell, _| {
        assert!(shell.visual.frame >= 1, "the window drew frames");
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
        assert_eq!(draw_frame(cx), 0);
    }
    entity.update(cx, |shell, _| {
        assert_eq!(shell.visual.frame, 0);
        assert!(shell.visual.probe.empty());
        assert!(shell.panes[0].as_ref().unwrap().list.borrow().probe.is_none());
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
    draw_frame(cx);
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

fn apply(
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

fn land_rows(entity: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext, path: &str, total: usize) {
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let count = shell.panes[0].as_ref().unwrap().pane.first_count().min(total);
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

fn expire(cx: &mut gpui::VisualTestContext, receive: &std::sync::mpsc::Receiver<Answer>) {
    for _ in 0..3 {
        draw_frame(cx);
    }
    assert!(receive.try_recv().is_err(), "no receipt for a frame that cannot draw the scene");
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
    expire(cx, &receive);
    entity.update(cx, |shell, _| assert!(shell.visual.receipt.is_none()));
}

#[gpui::test]
fn the_receipt_text_size_is_the_size_the_rows_drew(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    entity.update(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().list.borrow_mut().typeface.body = px(16.);
    });
    let receive = apply(&entity, cx, scene_spec("/files", "text_size", "14"));
    expire(cx, &receive);
    entity.update(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().list.borrow_mut().typeface.body = px(16.);
    });
    let receive = apply(&entity, cx, scene_spec("/files", "text_size", "16"));
    draw_frame(cx);
    let receipt = flea::jsondoc::parse(&receive.try_recv().unwrap().reply.stdout).unwrap();
    assert_eq!(receipt.get("text_size").and_then(Json::as_f64), Some(16.));
}

#[gpui::test]
fn a_cursor_after_a_path_change_is_drawn_by_the_receipt_frame(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    let receive = apply(&entity, cx, scene_spec("/other", "cursor", "5"));
    draw_frame(cx);
    assert!(receive.try_recv().is_err(), "rows have not landed");
    land_rows(&entity, cx, "/other", 40);
    let mut reply = None;
    for _ in 0..4 {
        // Run the callbacks of the frame drawn last, then read the cursor that frame's rows drew.
        let (answer, drawn) = cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            let drawn = entity.read(cx).panes[0].as_ref().unwrap().list.borrow().drawn_cursor;
            (receive.try_recv().ok(), drawn)
        });
        if let Some(answer) = answer {
            assert_eq!(drawn, Some(5), "the frame the receipt names drew the scene's cursor");
            reply = Some(answer.reply);
            break;
        }
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let reply = reply.expect("a receipt once the rows land");
    assert_eq!(reply.code, 0, "{reply:?}");
    let receipt = flea::jsondoc::parse(&reply.stdout).unwrap();
    assert_eq!(receipt.get("cursor").and_then(Json::as_f64), Some(5.));
    assert_eq!(receipt.get("path"), Some(&Json::Str("/other".into())));
    entity.update(cx, |shell, _| assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 5));
}
