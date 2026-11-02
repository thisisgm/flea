use super::*;

// EY-468: one line the backend writes that is not protocol costs that line only; the listing stays, as v0.3.12 does.
#[gpui::test]
fn ey468_one_garbled_backend_line_keeps_the_listing_and_selection(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| crate::state::tests::shell_with(backend, cx));
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.cursor = 3;
            host.selected = vec![3];
        })
        .unwrap();
    assert_eq!(read(cx, handle, "state"), Reply::value("ready"));
    let before = handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().error.clone()
        })
        .unwrap();
    handle
        .update(cx, |shell, window, cx| {
            let garbled = Backend::test_wire_line("harness: this line is not backend protocol");
            shell.receive(0, garbled, window, cx);
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(read(cx, handle, "state"), Reply::value("ready"));
    assert_eq!(
        read(cx, handle, "lastMessage"),
        Reply::value("That action could not be completed; try again.")
    );
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.held.total, 1000);
            assert_eq!(host.pane.cursor, 3);
            assert_eq!(host.selected, vec![3]);
            assert_eq!(host.error, before, "the pane's own error is not replaced");
        })
        .unwrap();
}

// EY-468 garble@list: the line lands while the list is out; v0.3.12 still lands the pair that follows it.
#[gpui::test]
fn ey468_a_garbled_line_after_the_list_request_keeps_the_listing(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| crate::state::tests::shell_with(backend, cx));
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            assert!(pane.begin("/next".into(), Default::default(), pane.now()));
            shell.receive(0, Backend::test_wire_line("garbled"), window, cx);
            assert!(
                shell.panes[0].as_ref().unwrap().pane.busy(),
                "the list stays out"
            );
            let rows = vec![flea::protocol::RowFacts::default(); 10];
            for reply in [
                flea::protocol::Reply::Listed {
                    path: "/next".into(),
                    total: 10,
                },
                flea::protocol::Reply::Rows {
                    start: 0,
                    rows,
                    listing: 3,
                },
            ] {
                shell.receive(0, crate::backend::Event::Reply(reply), window, cx);
            }
        })
        .unwrap();
    for _ in 0..4 {
        frame(cx, handle);
    }
    assert_eq!(read(cx, handle, "state"), Reply::value("ready"));
    assert_eq!(
        read(cx, handle, "lastMessage"),
        Reply::value("That action could not be completed; try again.")
    );
    handle
        .update(cx, |shell, _, _| {
            let pane = &shell.panes[0].as_ref().unwrap().pane;
            assert_eq!((pane.path(), pane.held.total), ("/next", 10));
        })
        .unwrap();
}

#[test]
fn ey468_garbled_diagnostics_survive_unwritable_stderr() {
    let stderr = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("ey468_one_garbled_backend_line_keeps_the_listing_and_selection")
        .arg("--nocapture")
        .stderr(stderr)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "recoverable garble with failed stderr: {stdout}"
    );
    assert!(
        stdout.contains("1 passed; 0 failed"),
        "the child exercised the real event handler: {stdout}"
    );
}
