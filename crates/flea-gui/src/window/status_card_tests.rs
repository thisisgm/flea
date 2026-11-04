use super::*;
use crate::window::root::Root;
use gpui::AppContext;

#[path = "status_transfer_tests.rs"]
mod transfer;

#[path = "status_card_rate_tests.rs"]
mod rate;

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
    let (backend, mut requests) = Backend::test_lines();
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
        Some("5.3 MB of 9.7 MB · 0 B/s")
    );
    assert!(!card.get("rect").unwrap().as_str().unwrap().is_empty());
    // EY-190: the controls carry Ipc.qml controlState's names.
    for (control, name) in [("cancel", "Cancel"), ("undo", "z undoes")] {
        assert_eq!(
            card.get(control)
                .and_then(|c| c.get("name"))
                .and_then(flea::jsondoc::Json::as_str),
            Some(name)
        );
    }
    assert_eq!(
        card.get("cancel")
            .unwrap()
            .get("enabled")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    // A running row offers no Dismiss, so the reader's enabled flag follows the row's own buttons.
    assert_eq!(
        card.get("dismiss")
            .unwrap()
            .get("enabled")
            .unwrap()
            .as_bool(),
        Some(false)
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
    while requests().is_some() {}
    gpui::VisualTestContext::from_window(handle.into(), cx).simulate_click(
        gpui::point(gpui::px(centre[0]), gpui::px(centre[1])),
        gpui::Modifiers::none(),
    );
    let cancel = requests().expect("the painted Cancel reaches its backend");
    let value = flea::jsondoc::parse(&cancel).unwrap();
    assert_eq!(
        value.get("c").and_then(flea::jsondoc::Json::as_str),
        Some("jobs")
    );
    assert_eq!(
        value.get("op").and_then(flea::jsondoc::Json::as_str),
        Some("cancel")
    );
    assert_eq!(cancel, r#"{"c":"jobs","op":"cancel","id":12}"#);
    assert!(requests().is_none(), "one press sends one cancellation");
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

#[gpui::test]
fn uish_misc_failed_job_exposes_its_painted_dismiss(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let handle = cx.add_window(|_, cx| Root {
        shell: cx.new(|cx| crate::state::tests::shell_with(backend, cx)),
    });
    handle.update(cx, |root, window, cx| root.shell.update(cx, |shell, cx| {
        shell.open_overlay_copy_to(true, "/files", window, cx);
        shell.overlay.as_ref().unwrap().update(cx, |overlay, cx| {
            overlay.model.dialog = None;
            assert!(overlay.line(r#"{"t":"job","job":{"id":13,"kind":"move","head":"Move failed","file":"a-marked.txt","done":0,"total":1,"bytes":0,"totalBytes":0,"itemBytes":0,"rate":0,"left":null,"state":"failed","reason":"permission denied","output":[],"undo":false,"cancelled":false},"unfinished":0}"#, cx));
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
    let dismiss = card.get("dismiss").unwrap();
    assert_eq!(dismiss.get("enabled").unwrap().as_bool(), Some(true));
    assert_eq!(dismiss.get("label").unwrap().as_str(), Some("Dismiss"));
    assert_eq!(
        card.get("cancel")
            .unwrap()
            .get("enabled")
            .unwrap()
            .as_bool(),
        Some(false)
    );
    let centre = dismiss
        .get("centre")
        .unwrap()
        .as_str()
        .unwrap()
        .split_whitespace()
        .map(|n| n.parse::<f32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(centre.len(), 2);
    while requests().is_some() {}
    gpui::VisualTestContext::from_window(handle.into(), cx).simulate_click(
        gpui::point(gpui::px(centre[0]), gpui::px(centre[1])),
        gpui::Modifiers::none(),
    );
    assert_eq!(
        requests().as_deref(),
        Some(r#"{"c":"jobs","op":"dismiss","id":13}"#),
        "the painted Dismiss reaches its backend"
    );
}
