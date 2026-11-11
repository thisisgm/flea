//! A finished Jobs card leaves Escape to the footer without losing its Retry action.
use super::*;

#[gpui::test]
fn uish_misc_failed_jobs_card_keeps_keyboard_menu_available(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let handle = open_with(cx, backend);
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":13,"kind":"move","head":"Move failed","state":"failed","reason":"permission denied"}}"#,
    );
    advance(cx, handle, 150);
    assert!(drawn(cx, handle), "the failed Jobs card is drawn");
    handle
        .update(cx, |root, window, cx| {
            assert!(
                root.shell.read(cx).focus.contains_focused(window, cx),
                "the non-modal Jobs card leaves actual listing focus: {:?}",
                window.focused(cx)
            );
        })
        .unwrap();
    let _sent: Vec<_> = std::iter::from_fn(&mut requests).collect();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("menu");
    visual.run_until_parked();
    let sent: Vec<_> = std::iter::from_fn(&mut requests).collect();
    handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            assert!(
                shell.menu.captured.is_some(),
                "the non-modal Jobs card permits the listing's keyboard menu: context={:?} blocked={} modal={} busy={} stopped={} sent={sent:?}",
                shell.keyboard.context,
                shell.wheel_blocked(cx),
                shell.modal_card_holds_keyboard(cx),
                shell.panes[0].as_ref().unwrap().pane.busy(),
                shell.panes[0].as_ref().unwrap().stopped,
            );
        })
        .unwrap();
    assert!(
        sent.iter().any(|line| line.contains(r#""op":"snapshot""#)),
        "the real menu requests a new snapshot: {sent:?}"
    );
}

#[gpui::test]
fn uish_misc_failed_jobs_card_allows_footer_error_acknowledgement(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    for event in [
        r#"{"t":"transferstarted","id":13,"n":1,"moving":true}"#,
        r#"{"t":"job","job":{"id":13,"kind":"move","head":"Moving a.txt","state":"running"}}"#,
    ] {
        line(cx, handle, event);
    }
    advance(cx, handle, 150);
    assert!(drawn(cx, handle));
    advance(cx, handle, 100);
    for event in [
        r#"{"t":"transferitem","id":13,"index":0,"name":"a.txt","ok":false,"err":"permission denied"}"#,
        r#"{"t":"job","job":{"id":13,"kind":"move","head":"Move failed","state":"failed","reason":"permission denied"}}"#,
        r#"{"t":"transferdone","id":13,"ok":0,"failed":1}"#,
    ] {
        line(cx, handle, event);
    }
    advance(cx, handle, 100);
    assert!(drawn(cx, handle), "the failed card remains drawn");
    let errors = |cx: &mut TestAppContext| {
        handle
            .update(cx, |root, _, cx| {
                root.shell.update(cx, |shell, cx| {
                    let mut errors = 0;
                    shell.status_messages(cx, |messages, _| errors = messages.errors().len());
                    errors
                })
            })
            .unwrap()
    };
    let before = errors(cx);
    assert!(before > 0, "the actual transfer failure reaches the footer");
    let _sent: Vec<_> = std::iter::from_fn(&mut requests).collect();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert_eq!(
        errors(cx),
        before - 1,
        "Escape acknowledges one footer error"
    );
    assert!(
        drawn(cx, handle),
        "acknowledgement keeps the failed Jobs card"
    );
    handle
        .update(cx, |root, _, cx| {
            let overlay = root.shell.read(cx).operation_view(0).unwrap();
            assert!(overlay
                .read(cx)
                .model
                .jobs
                .rows
                .iter()
                .any(|row| row.id == 13 && row.state == "failed"));
        })
        .unwrap();
    let sent: Vec<_> = std::iter::from_fn(&mut requests).collect();
    assert!(
        !sent.iter().any(|line| line.contains("cancel")),
        "a completed failure sends no cancel: {sent:?}"
    );
}
