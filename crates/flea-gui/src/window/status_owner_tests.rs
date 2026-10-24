use super::{
    cards::{draw, state},
    *,
};
use crate::{
    backend::{Event as BackendEvent, Failure},
    window::{pane::PaneHost, root::Root},
};
use gpui::AppContext;

fn death(cx: &mut TestAppContext, failed: bool, dead: usize) {
    let (backend, _requests) = Backend::test_channel();
    let (other_backend, _other_requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| Root {
        shell: cx.new(|cx| crate::state::tests::shell_with(backend, cx)),
    });
    handle.update(cx, |root, window, cx| root.shell.update(cx, |shell, cx| {
        let mut other = PaneHost::new("/other".into(), None, 620., &shell.locale);
        other.backend = Some(other_backend);
        shell.panes[1] = Some(other);
        shell.lifecycle.attach(1);
        shell.status_messages(cx, |messages, now| {
            messages.set_activity(1, "Moving file", 12, true, false, now);
            assert_eq!(messages.cancel_transfer(), Some(12));
            messages.set_activity(0, "Copying file", 12, true, false, now);
        });
        shell.open_overlay_copy_to(false, "/files", window, cx);
        shell.overlay.as_ref().unwrap().update(cx, |overlay, cx| {
            overlay.model.dialog = None;
            assert!(overlay.line(r#"{"t":"transferstarted","id":12,"n":2,"moving":false}"#, cx));
            assert!(overlay.line(r#"{"t":"transferprogress","id":12,"phase":"writing","drive":"NAS"}"#, cx));
            assert!(overlay.line(r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copying file","file":"a.bin","done":1,"total":2,"bytes":1,"totalBytes":2,"state":"running"},"unfinished":1}"#, cx));
        });
    })).unwrap();
    cx.run_until_parked();
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::HOLD_MS));
    cx.run_until_parked();
    draw(cx, handle);
    let before = state(cx, handle);
    let card = before.get("transferCard").unwrap();
    assert_eq!(card.get("writing").unwrap().as_bool(), Some(true));
    assert_eq!(card.get("cancelling").unwrap().as_bool(), Some(false));
    assert_eq!(
        before.get("activities").unwrap().as_array().unwrap().len(),
        2
    );
    assert_eq!(
        before
            .get("transferCard")
            .unwrap()
            .get("visible")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                let event = if failed {
                    BackendEvent::Failed(Failure {
                        key: "window.backend_eof",
                        detail: String::new(),
                    })
                } else {
                    BackendEvent::Drained
                };
                shell.receive(dead, event, window, cx);
            })
        })
        .unwrap();
    draw(cx, handle);
    let after = state(cx, handle);
    let activities = after.get("activities").unwrap().as_array().unwrap();
    assert_eq!(activities.len(), 1);
    assert_eq!(
        activities[0].get("ownerPath").unwrap().as_str(),
        Some(if dead == 0 { "/other" } else { "/files" })
    );
    assert_eq!(activities[0].get("running").unwrap().as_bool(), Some(true));
    assert_eq!(
        activities[0].get("cancelling").unwrap().as_bool(),
        Some(dead == 0)
    );
    let card = after.get("transferCard").unwrap();
    assert_eq!(card.get("visible").unwrap().as_bool(), Some(dead != 0));
    assert_eq!(card.get("writing").unwrap().as_bool(), Some(dead != 0));
    assert_eq!(card.get("cancelling").unwrap().as_bool(), Some(false));
    assert_eq!(
        card.get("headline").unwrap().as_str(),
        Some(if dead == 0 { "" } else { "Copying file" })
    );
    assert_eq!(
        after.get("errors").unwrap().as_f64(),
        Some(if failed { 1. } else { 0. })
    );
    if failed {
        handle
            .update(cx, |root, window, cx| {
                root.shell.update(cx, |shell, cx| {
                    shell.receive(dead, BackendEvent::Drained, window, cx);
                })
            })
            .unwrap();
        draw(cx, handle);
        assert_eq!(state(cx, handle).get("activities"), after.get("activities"));
    }
}

#[gpui::test]
fn u05_backend_error_retires_only_dead_panes_activity_and_card(cx: &mut TestAppContext) {
    for dead in [0, 1] {
        death(cx, true, dead);
    }
}

#[gpui::test]
fn u05_backend_drained_without_completion_retires_only_dead_pane(cx: &mut TestAppContext) {
    for dead in [0, 1] {
        death(cx, false, dead);
    }
}
