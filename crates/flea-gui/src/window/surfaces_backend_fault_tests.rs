use super::*;

fn call(cx: &mut TestAppContext, handle: WindowHandle<Shell>, function: &str) -> String {
    use flea::ipc::wire::Request;
    handle
        .update(cx, |shell, window, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                crate::ipc::server::Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: function.into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            );
            receive.try_recv().unwrap().reply.stdout.trim().to_owned()
        })
        .expect("the shell window is open")
}

// EY-465: a backend that never answered leaves the path bar and the disk fact empty, and the bar says its sentence alone.
#[gpui::test]
fn ey465_a_backend_that_fails_before_any_listing_says_its_sentence_alone(cx: &mut TestAppContext) {
    let handle = open_startup(cx);
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                BackendEvent::Failed(Failure {
                    key: "window.backend_eof",
                    detail: String::new(),
                }),
                window,
                cx,
            );
        })
        .expect("the shell window is open");
    frame(cx, handle);
    frame(cx, handle);
    let generic = catalog_sentence(cx, handle, "feedback.error.generic");
    assert_eq!(call(cx, handle, "lastMessage"), generic);
    assert_eq!(
        facts(cx, handle).map(|facts| facts.sentence.to_string()),
        Some(generic),
        "the state slot draws the same sentence"
    );
    assert_eq!(
        call(cx, handle, "statusSecondary"),
        "",
        "no Esc hint under the pane's own sentence"
    );
    let (label, disk) = handle
        .update(cx, |shell, _, cx| {
            let disk = shell.footer(cx).read(cx).filesystem.to_string();
            (shell.path_labels[0].to_string(), disk)
        })
        .expect("the shell window is open");
    assert_eq!(
        label, "",
        "the path bar stays empty: no listing ever named a folder"
    );
    assert_eq!(disk, "", "no disk fact was ever answered, so none is drawn");
}
