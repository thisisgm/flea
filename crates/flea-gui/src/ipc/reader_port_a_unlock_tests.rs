//! unlockState through the mounted card: its painted geometry, its buttons' pointer feedback and a reopened card.
use super::live::network;
use super::*;

#[gpui::test]
fn ipc_unlock_answers_the_painted_card(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    network(cx, handle);
    update(cx, handle, |shell, window, cx| {
        shell.open_unlock(
            0,
            "/dev/unit-only",
            "Test drive",
            1024,
            "crypto_LUKS",
            window,
            cx,
        )
    });
    // EY-325: the open card answers from its model and its last paint, so the lane's waits can pass.
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    let reply = read(cx, handle, "unlockState");
    assert_eq!(reply.code, 0, "{reply:?}");
    let state = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
    let text = |key: &str| {
        state
            .get(key)
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("")
            .to_owned()
    };
    assert_eq!(
        state.get("opened").and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
    assert_eq!(text("phase"), "typing");
    assert_eq!(text("device"), "/dev/unit-only");
    assert_eq!(
        state
            .get("noteIsError")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    assert_eq!(text("rect").split(' ').count(), 4, "{}", reply.stdout);
    assert_eq!(text("eye").split(' ').count(), 2, "{}", reply.stdout);
    let controls = state
        .get("controls")
        .and_then(flea::jsondoc::Json::as_array)
        .unwrap();
    assert_eq!(controls.len(), 2);
    assert_eq!(
        controls[1]
            .get("enabled")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(false),
        "an empty field leaves Unlock unavailable"
    );
    assert!(!controls[0]
        .get("rect")
        .and_then(flea::jsondoc::Json::as_str)
        .unwrap()
        .is_empty());
    // Cancel's own handlers: hovered and pressed while held on it, released when the press leaves.
    let centre = controls[0]
        .get("centre")
        .and_then(flea::jsondoc::Json::as_str)
        .unwrap()
        .to_owned();
    let (x, y) = centre.split_once(' ').unwrap();
    let at = gpui::point(gpui::px(x.parse().unwrap()), gpui::px(y.parse().unwrap()));
    let none = gpui::Modifiers::none();
    let mut visual = gpui::VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_move(at, None, none);
    visual.simulate_mouse_down(at, gpui::MouseButton::Left, none);
    let cancel = |cx: &mut TestAppContext, key: &str| {
        let reply = read(cx, handle, "unlockState");
        let state = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
        state
            .get("controls")
            .and_then(flea::jsondoc::Json::as_array)
            .unwrap()[0]
            .get(key)
            .and_then(flea::jsondoc::Json::as_bool)
    };
    assert_eq!(cancel(cx, "hovered"), Some(true));
    assert_eq!(cancel(cx, "pressed"), Some(true));
    let away = gpui::point(gpui::px(1.), gpui::px(1.));
    visual.simulate_mouse_up(away, gpui::MouseButton::Left, none);
    assert_eq!(
        cancel(cx, "pressed"),
        Some(false),
        "a press released off the button"
    );
}

// unlockState inside the caller's own update, before the frame that update's end draws.
fn read_now(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) -> String {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    let request = Request::Call {
        target: "flea".into(),
        function: "unlockState".into(),
        arguments: vec![],
    };
    shell.ipc(
        crate::ipc::server::Event::Call { request, answer },
        window,
        cx,
    );
    receive.try_recv().unwrap().reply.stdout
}

#[gpui::test]
fn ipc_unlock_reopened_before_a_frame_reads_no_old_geometry(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    network(cx, handle);
    update(cx, handle, |shell, window, cx| {
        shell.open_unlock(
            0,
            "/dev/unit-one",
            "Test drive",
            1024,
            "crypto_LUKS",
            window,
            cx,
        )
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    // Each reopen closes the painted card and opens another inside one update, then reads before any frame.
    for (device, by_device) in [("/dev/unit-two", false), ("/dev/unit-three", true)] {
        let reply = update(cx, handle, |shell, window, cx| {
            let unlock = shell.rail.as_ref().unwrap().unlock.as_ref().unwrap();
            unlock.view.clone().update(cx, |view, cx| view.close(cx));
            if by_device {
                shell.open_unlock_device(device, window, cx);
            } else {
                shell.open_unlock(0, device, "Test drive", 1024, "crypto_LUKS", window, cx);
            }
            read_now(shell, window, cx)
        });
        let state = flea::jsondoc::parse(reply.trim()).unwrap();
        let text = |key: &str| state.get(key).and_then(flea::jsondoc::Json::as_str);
        assert_eq!(text("device"), Some(device), "{reply}");
        assert_eq!(text("rect"), Some(""), "{reply}");
        assert_eq!(text("eye"), Some(""), "{reply}");
    }
}
