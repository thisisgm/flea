//! A typed bare host traverses the production service, then the mounted browser opens a child share.
use super::*;

fn reply(
    recorder: &Recorder,
    command: &str,
    uri: &str,
    code: i32,
    text: &str,
    cx: &mut VisualTestContext,
) {
    let index = recorder
        .jobs
        .borrow()
        .iter()
        .rposition(|job| {
            job.argv.first().map(String::as_str) == Some("gio")
                && job.argv.get(1).map(String::as_str) == Some(command)
                && job.argv.last().map(String::as_str) == Some(uri)
        })
        .unwrap_or_else(|| panic!("missing gio {command} for {uri}"));
    recorder.answer(index, code, text);
    cx.run_until_parked();
}

#[gpui::test]
fn vf3_typed_host_service_paints_browser_and_activates_the_picked_share(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open(recorder.clone(), cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.network_address(0, "smb://nas/", window, cx);
        })
    });
    cx.run_until_parked();
    reply(&recorder, "mount", "smb://nas/", 0, "", cx);
    reply(&recorder, "info", "smb://nas/", 1, "", cx);
    // A pane state surface is withheld while the browser covers its pane, as v0.3.12 hides it.
    shell.update(cx, |shell, cx| {
        shell.slots.states[0] = Some(
            cx.new(|_| crate::g2::status_feedback::state::StateMessage::new())
                .into(),
        );
        assert!(shell.pane_state_view(0, cx).is_some());
    });
    reply(&recorder, "list", "smb://nas/", 0, "Docs\nMedia\n", cx);
    frame(cx);
    shell.update(cx, |shell, cx| {
        assert!(shell.pane_state_view(0, cx).is_none())
    });
    assert!(
        cx.debug_bounds("share-motion").is_some(),
        "typed host's production overlay must paint"
    );
    shell.read_with(cx, |shell, cx| {
        let browser = shell
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .shares
            .read(cx);
        assert!(browser.model.open);
        assert_eq!(browser.model.names, ["Docs", "Media"]);
    });
    cx.simulate_keystrokes("down");
    frame(cx);
    shell.read_with(cx, |shell, cx| {
        assert_eq!(
            shell
                .rail
                .as_ref()
                .unwrap()
                .network
                .as_ref()
                .unwrap()
                .shares
                .read(cx)
                .model
                .cursor,
            1
        )
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        recorder
            .jobs
            .borrow()
            .iter()
            .any(|job| job.argv.get(1).map(String::as_str) == Some("mount")
                && job.argv.last().map(|uri| uri.trim_end_matches('/')) == Some("smb://nas/Media")),
        "the picked share must use the existing mount route: {:?}",
        recorder
            .jobs
            .borrow()
            .iter()
            .map(|job| &job.argv)
            .collect::<Vec<_>>()
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
