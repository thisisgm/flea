//! A saved connection refused from its Edit Address card says so in the footer, and the line outlives the card (v0.3.12 failMount).
use super::*;

fn read(shell: &Entity<Shell>, name: &str, cx: &mut VisualTestContext) -> String {
    let (answer, receiver) = std::sync::mpsc::sync_channel(1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.ipc(
                crate::ipc::server::Event::Call {
                    request: flea::ipc::wire::Request::Call {
                        target: "flea".into(),
                        function: name.into(),
                        arguments: Vec::new(),
                    },
                    answer,
                },
                window,
                cx,
            )
        })
    });
    let reply = receiver.try_recv().expect("a synchronous IPC answer").reply;
    assert_eq!(reply.code, 0, "{name} is served: {reply:?}");
    reply.stdout.trim_end_matches('\n').to_string()
}

#[gpui::test]
fn fix_fileops_2_refused_saved_connection_keeps_the_footer_line(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    let card = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.network().unwrap().bookmarks =
                PathBuf::from("/nonexistent/fix-fileops-2/bookmarks");
            let origin = shell.network_origin(0);
            let card = shell.network().unwrap().card.clone();
            card.update(cx, |card, cx| {
                assert!(card.location(
                    "smb://127.0.0.1/flea",
                    "Fixture share",
                    true,
                    origin,
                    window,
                    cx
                ));
            });
            card
        })
    });
    frame(cx);
    cx.simulate_keystrokes("enter");
    frame(cx);
    let mount = *recorder.mounts().last().expect("Retry runs gio mount");
    recorder.answer(mount, 1, "");
    cx.run_until_parked();
    let info = *recorder
        .find("info")
        .last()
        .expect("a failed mount asks gio info");
    recorder.answer(info, 1, "");
    frame(cx);
    let refused = shell.read_with(cx, |shell, _| shell.locale.catalog.tr("network.refused"));
    assert_eq!(
        read(&shell, "networkStatus", cx),
        refused,
        "the card says it"
    );
    assert_eq!(
        read(&shell, "lastMessage", cx),
        refused,
        "the footer says it"
    );
    assert_eq!(read(&shell, "statusError", cx), "true");
    cx.update(|_, cx| card.update(cx, |card, cx| card.close(cx)));
    frame(cx);
    assert!(!cx.update(|_, cx| card.read(cx).model.opened));
    assert_eq!(
        read(&shell, "statusPrimary", cx),
        refused,
        "the line outlives the card"
    );
    assert_eq!(read(&shell, "statusError", cx), "true");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
