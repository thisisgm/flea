use super::*;
use crate::window::root::Root;
use gpui::AppContext;

pub(super) fn state(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> flea::jsondoc::Json {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                let (answer, receive) = std::sync::mpsc::sync_channel(1);
                shell.ipc(
                    Event::Call {
                        request: Request::Call {
                            target: "flea".into(),
                            function: "statusActivityState".into(),
                            arguments: vec![],
                        },
                        answer,
                    },
                    window,
                    cx,
                );
                let reply = receive.try_recv().unwrap().reply;
                assert_eq!(reply.code, 0);
                flea::jsondoc::parse(reply.stdout.trim()).unwrap()
            })
        })
        .unwrap()
}

pub(super) fn draw(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

#[gpui::test]
fn u05_transfer_observer_reads_the_painted_job_block_and_its_fold(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| Root {
        shell: cx.new(|cx| crate::state::tests::shell_with(backend, cx)),
    });
    draw(cx, handle);
    assert_eq!(
        state(cx, handle)
            .get("transferCard")
            .unwrap()
            .get("visible")
            .unwrap()
            .as_bool(),
        Some(false)
    );
    handle.update(cx, |root, window, cx| root.shell.update(cx, |shell, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx);
        shell.overlay.as_ref().unwrap().update(cx, |overlay, cx| {
            overlay.model.dialog = None;
            assert!(overlay.line(r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copying 10 of 18 to NAS","file":"a.bin","done":9,"total":18,"bytes":5300000,"totalBytes":9700000,"itemBytes":458300,"rate":1100000,"left":4,"state":"running","reason":"","output":[],"undo":false,"cancelled":false},"unfinished":1}"#, cx));
        });
    })).unwrap();
    cx.run_until_parked();
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::HOLD_MS));
    cx.run_until_parked();
    draw(cx, handle);
    let seen = state(cx, handle);
    let card = seen.get("transferCard").unwrap();
    assert_eq!(card.get("visible").unwrap().as_bool(), Some(true));
    assert_eq!(
        card.get("headline").unwrap().as_str(),
        Some("Copying 10 of 18 to NAS")
    );
    assert_eq!(
        card.get("byteLine").unwrap().as_str(),
        Some("5.3 MB of 9.7 MB · 1.1 MB/s · 0:04 left")
    );
    assert!(!card.get("rect").unwrap().as_str().unwrap().is_empty());
    assert_eq!(
        card.get("cancel")
            .unwrap()
            .get("enabled")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    let centre = card
        .get("cancel")
        .unwrap()
        .get("centre")
        .unwrap()
        .as_str()
        .unwrap()
        .split_whitespace()
        .map(|n| n.parse::<f32>().unwrap())
        .collect::<Vec<_>>();
    let bounds = card
        .get("rect")
        .unwrap()
        .as_str()
        .unwrap()
        .split_whitespace()
        .map(|n| n.parse::<f32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(centre.len(), 2);
    assert_eq!(bounds.len(), 4);
    assert!(centre[0] >= bounds[0] && centre[0] <= bounds[0] + bounds[2]);
    assert!(centre[1] >= bounds[1] && centre[1] <= bounds[1] + bounds[3]);
    gpui::VisualTestContext::from_window(handle.into(), cx).simulate_click(
        gpui::point(gpui::px(centre[0]), gpui::px(centre[1])),
        gpui::Modifiers::none(),
    );
    handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, cx| {
                assert!(shell
                    .overlay
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .model
                    .outbox
                    .iter()
                    .any(|line| {
                        let value = flea::jsondoc::parse(line).unwrap();
                        value.get("c").and_then(flea::jsondoc::Json::as_str) == Some("jobs")
                            && value.get("op").and_then(flea::jsondoc::Json::as_str)
                                == Some("cancel")
                    }));
            })
        })
        .unwrap();
    handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, cx| {
                shell
                    .overlay
                    .as_ref()
                    .unwrap()
                    .update(cx, |overlay, cx| overlay.fold_jobs(cx));
            })
        })
        .unwrap();
    draw(cx, handle);
    assert_eq!(
        state(cx, handle)
            .get("transferCard")
            .unwrap()
            .get("visible")
            .unwrap()
            .as_bool(),
        Some(false)
    );
}
