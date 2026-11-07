//! Compare the real committed frame's spiral paint with the production Shell IPC route.
use super::*;

fn agrees(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> bool {
    drawn(cx, handle);
    let mark = VisualTestContext::from_window(handle.into(), cx).debug_bounds("op1-job-13-mark");
    handle
        .update(cx, |root, window, cx| {
            let view = root.shell.read(cx).operation_view(0).unwrap();
            let frame = view.read(cx).palette.muted_frame;
            let painted = mark.is_some_and(|mark| {
                let square = mark.scale(window.scale_factor());
                window.painted_quads().iter().any(|quad| {
                    quad.bounds.is_contained_within(&square)
                        && quad
                            .background
                            .as_solid()
                            .is_some_and(|ink| (ink.h, ink.s, ink.l) == (frame.h, frame.s, frame.l))
                })
            });
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            root.shell.update(cx, |shell, cx| {
                shell.ipc(
                    crate::ipc::server::Event::Call {
                        request: flea::ipc::wire::Request::Call {
                            target: "flea".into(),
                            function: "omiState".into(),
                            arguments: vec![],
                        },
                        answer,
                    },
                    window,
                    cx,
                )
            });
            let reply = receive.try_recv().unwrap().reply;
            assert_eq!(reply.code, 0);
            let data = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
            let reported = data.as_array().unwrap().iter().any(|row| {
                row.get("id").and_then(flea::jsondoc::Json::as_f64) == Some(13.)
                    && row.get("shown").and_then(flea::jsondoc::Json::as_bool) == Some(true)
            });
            assert_eq!(
                reported, painted,
                "omiState must describe the committed spiral paint"
            );
            painted
        })
        .unwrap()
}

#[gpui::test]
fn omi_fast_job_and_stale_timer_never_report_an_unpainted_spiral(cx: &mut TestAppContext) {
    let (backend, _received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    start(cx, handle);
    assert!(!agrees(cx, handle));
    advance(cx, handle, 149);
    finish(cx, handle);
    assert!(!agrees(cx, handle));
    advance(cx, handle, 101);
    assert!(!agrees(cx, handle));
}

#[gpui::test]
fn omi_removed_job_keeps_the_drawn_owner_until_the_minimum_expires(cx: &mut TestAppContext) {
    let (backend, _received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    start(cx, handle);
    advance(cx, handle, 150);
    assert!(agrees(cx, handle));
    advance(cx, handle, 20);
    finish(cx, handle);
    line(cx, handle, r#"{"t":"jobs","jobs":[]}"#);
    assert!(agrees(cx, handle));
    advance(cx, handle, 79);
    assert!(agrees(cx, handle));
    advance(cx, handle, 1);
    assert!(!agrees(cx, handle));
}

#[gpui::test]
fn omi_fold_unfold_and_failed_frames_use_the_committed_jobs(cx: &mut TestAppContext) {
    let (backend, _received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    start(cx, handle);
    advance(cx, handle, 150);
    assert!(agrees(cx, handle));
    for expected_paint in [false, true] {
        handle
            .update(cx, |root, _, cx| {
                root.shell
                    .read(cx)
                    .operation_view(0)
                    .unwrap()
                    .update(cx, |overlay, cx| overlay.fold_jobs(cx));
            })
            .unwrap();
        assert_eq!(agrees(cx, handle), expected_paint);
    }
    advance(cx, handle, 100);
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":13,"kind":"copy","head":"Copy failed","state":"failed","reason":"Permission denied"}}"#,
    );
    line(
        cx,
        handle,
        r#"{"t":"transferdone","id":13,"ok":0,"failed":1}"#,
    );
    advance(cx, handle, 100);
    assert!(!agrees(cx, handle));
    assert!(
        VisualTestContext::from_window(handle.into(), cx)
            .debug_bounds("op1-job-13-mark")
            .is_some(),
        "failed job must still draw its alert"
    );
}
