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

// EY-546: with no folder ever listed there is nothing to go up from, so Up dims beside Back, as v0.3.12 draws them.
#[gpui::test]
fn ey546_a_backend_that_fails_before_any_listing_dims_up(cx: &mut TestAppContext) {
    let handle = open_startup(cx);
    frame(cx, handle);
    let drawn = |cx: &mut TestAppContext| {
        handle
            .update(cx, |shell, _, cx| {
                let chrome = shell.chrome.view.clone().expect("mounted chrome");
                chrome.read_with(cx, |view, _| (view.inputs.back, view.inputs.up))
            })
            .expect("the shell window is open")
    };
    assert_eq!(
        drawn(cx),
        (false, true),
        "the first list is out, so Up climbs from it"
    );
    handle
        .update(cx, |shell, window, cx| {
            let failure = Failure {
                key: "window.backend_eof",
                detail: String::new(),
            };
            shell.receive(0, BackendEvent::Failed(failure), window, cx);
        })
        .expect("the shell window is open");
    frame(cx, handle);
    frame(cx, handle);
    assert_eq!(
        drawn(cx),
        (false, false),
        "Back and Up both dim with no folder"
    );
}

// EY-546: a first listing refused for a named folder names the pane, so Up still climbs out of it as v0.3.12 sets path from the failure.
#[gpui::test]
fn ey546_a_first_listing_refused_for_its_folder_keeps_up(cx: &mut TestAppContext) {
    let handle = open_startup(cx);
    frame(cx, handle);
    let up = handle
        .update(cx, |shell, window, cx| {
            let refusal = BackendEvent::Refused {
                operation: "scan".into(),
                path: "/files".into(),
                message: "No such file or directory".into(),
                mode: 0,
            };
            shell.receive(0, refusal, window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert!(!host.pane.busy(), "the refusal ended the first listing");
            host.nav.can_go_up(&host.pane)
        })
        .expect("the shell window is open");
    assert!(up, "a folder the failure named is a folder to go up from");
}
