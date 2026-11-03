//! A direct Copy To mount cannot inherit another pane's operation-session identity.
use super::{focus_shell, frame, marks, open, shell};
use crate::{
    backend::{Backend, Event, Failure},
    g2::op1::model::Dialog,
    window::{pane::PaneHost, startup::View},
};
use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowHandle};

fn mounted_session(
    cx: &mut TestAppContext,
    side: usize,
) -> (
    WindowHandle<crate::window::root::Root>,
    impl FnMut() -> Option<String>,
    impl FnMut() -> Option<String>,
) {
    let (backend, requests) = Backend::test_lines();
    let (other, other_requests) = Backend::test_lines();
    let handle = open(cx, backend);
    shell(cx, handle, |shell, window, cx| {
        let mut host = PaneHost::new("/other".into(), None, 620., &shell.locale);
        host.backend = Some(other);
        host.pane.use_executor(cx.background_executor().clone());
        shell.panes[1] = Some(host);
        shell.lifecycle.attach(1);
        shell.set_view(View::Dual, window, cx);
        for reply in [
            flea::protocol::Reply::Listed {
                path: "/other".into(),
                total: 1,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                rows: vec![Default::default()],
                listing: 3,
            },
        ] {
            shell.receive(1, Event::Reply(reply), window, cx);
        }
        assert_eq!(
            shell.view,
            View::Dual,
            "the secondary's real replies must land Dual"
        );
    });
    focus_shell(cx, handle);
    frame(cx, handle);
    shell(cx, handle, |shell, window, cx| {
        shell.focus_pane(side, cx);
        assert_eq!(
            shell.focus_side, side,
            "the session's requested pane must really focus"
        );
        shell.copy_to_operation(side, 71, false, "/source", window, cx);
        shell.receive(
            side,
            Event::Operations(
                r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copying file","file":"a.bin","done":1,"total":2,"bytes":1,"totalBytes":2,"state":"running"},"unfinished":1}"#.into(),
            ),
            window,
            cx,
        );
    });
    frame(cx, handle);
    assert!(marks(cx, handle, "op1-scrim").is_some());
    shell(cx, handle, |shell, _, cx| {
        let session = shell.operation_view(side).unwrap();
        assert_eq!(shell.overlay.as_ref(), Some(&session));
        assert_eq!(session.read(cx).owner, Some(side));
        assert!(matches!(
            session.read(cx).model.dialog,
            Some(Dialog::CopyTo { menu: 71, .. })
        ));
        assert_eq!(session.read(cx).model.jobs.rows.len(), 1);
        assert_eq!(session.read(cx).model.jobs.rows[0].state, "running");
    });
    (handle, requests, other_requests)
}

fn cancel_card(cx: &mut TestAppContext, handle: WindowHandle<crate::window::root::Root>) {
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    let cancel = visual.debug_bounds("op1-button-0").unwrap().center();
    visual.simulate_click(cancel, Modifiers::none());
    visual.run_until_parked();
    frame(cx, handle);
    assert!(marks(cx, handle, "op1-scrim").is_none());
}

fn death(failed: bool) -> Event {
    if failed {
        Event::Failed(Failure {
            key: "window.backend_eof",
            detail: String::new(),
        })
    } else {
        Event::Drained
    }
}

fn other_owner_survives(cx: &mut TestAppContext, failed: bool) {
    for dead in [0, 1] {
        let (handle, mut first_lines, mut second_lines) = mounted_session(cx, dead);
        let old = shell(cx, handle, |shell, _, _| {
            shell.operation_view(dead).unwrap()
        });
        cancel_card(cx, handle);
        let first: Vec<_> = std::iter::from_fn(&mut first_lines).collect();
        let second: Vec<_> = std::iter::from_fn(&mut second_lines).collect();
        let closed =
            |line: &String| line.contains("\"id\":71") && line.contains("\"op\":\"close\"");
        assert_eq!(first.iter().any(closed), dead == 0);
        assert_eq!(second.iter().any(closed), dead == 1);
        shell(cx, handle, |shell, _, cx| {
            assert_eq!(shell.overlay.as_ref(), Some(&old));
            assert!(old.read(cx).model.dialog.is_none());
            assert_eq!(old.read(cx).model.jobs.rows[0].state, "running");
        });
        let survivor = 1 - dead;
        shell(cx, handle, |shell, window, cx| {
            shell.focus_pane(survivor, cx);
            assert_eq!(shell.view, View::Dual);
            assert_eq!(
                shell.focus_side, survivor,
                "the Copy To must originate from the other pane"
            );
            shell.open_overlay_copy_to(false, "/destination", window, cx);
        });
        frame(cx, handle);
        assert!(marks(cx, handle, "op1-scrim").is_some());
        let (live, pending, focus) = shell(cx, handle, |shell, window, cx| {
            let live = shell.overlay.clone().unwrap();
            assert_eq!(
                live.read(cx).owner,
                Some(survivor),
                "verify the real card owner before death"
            );
            let pending = live.read(cx).model.dialog.clone();
            assert!(matches!(&pending, Some(Dialog::CopyTo { menu: 0, .. })));
            let focus = live.read(cx).focus_handle().unwrap();
            assert!(
                focus.is_focused(window),
                "the new card owns focus before backend death"
            );
            (live, pending, focus)
        });
        shell(cx, handle, |shell, window, cx| {
            shell.receive(dead, death(failed), window, cx);
        });
        frame(cx, handle);
        shell(cx, handle, |shell, window, cx| {
            assert_eq!(
                live.read(cx).model.dialog,
                pending,
                "old backend death cleared the other owner's Copy To dialog"
            );
            assert_eq!(shell.overlay.as_ref(), Some(&live));
            assert_eq!(live.read(cx).owner, Some(survivor));
            assert!(
                focus.is_focused(window),
                "old backend death stole the new card's focus"
            );
            assert_eq!(shell.operation_view(dead).as_ref(), Some(&old));
            assert_ne!(
                old, live,
                "the old operation session must remain a distinct entity"
            );
            assert_eq!(old.read(cx).owner, Some(dead));
            assert!(old.read(cx).model.dialog.is_none());
            assert_eq!(old.read(cx).model.jobs.rows[0].state, "failed");
            assert!(!old.read(cx).model.wire.transfer.running);
        });
        assert!(marks(cx, handle, "op1-scrim").is_some());
        if failed {
            shell(cx, handle, |shell, window, cx| {
                shell.receive(dead, Event::Drained, window, cx);
            });
            frame(cx, handle);
            shell(cx, handle, |shell, window, cx| {
                assert_eq!(live.read(cx).model.dialog, pending);
                assert_eq!(shell.overlay.as_ref(), Some(&live));
                assert!(focus.is_focused(window));
            });
        }
    }
}

#[gpui::test]
fn g70_direct_copy_keeps_its_owner_after_other_backend_failed(cx: &mut TestAppContext) {
    other_owner_survives(cx, true);
}

#[gpui::test]
fn g70_direct_copy_keeps_its_owner_after_other_backend_drained(cx: &mut TestAppContext) {
    other_owner_survives(cx, false);
}

#[gpui::test]
fn g70_direct_copy_reuses_same_owner_and_its_session_cleanup(cx: &mut TestAppContext) {
    for failed in [false, true] {
        let side = 0;
        let (handle, mut first_lines, mut second_lines) = mounted_session(cx, side);
        let old = shell(cx, handle, |shell, _, _| {
            shell.operation_view(side).unwrap()
        });
        cancel_card(cx, handle);
        let first: Vec<_> = std::iter::from_fn(&mut first_lines).collect();
        let second: Vec<_> = std::iter::from_fn(&mut second_lines).collect();
        assert!(first
            .iter()
            .any(|line| { line.contains("\"id\":71") && line.contains("\"op\":\"close\"") }));
        assert!(!second.iter().any(|line| line.contains("\"op\":\"close\"")));
        shell(cx, handle, |shell, window, cx| {
            shell.open_overlay_copy_to(false, "/again", window, cx);
            assert_eq!(
                shell.overlay.as_ref(),
                Some(&old),
                "same-owner session must be reused"
            );
            assert_eq!(shell.operation_view(side).as_ref(), Some(&old));
            assert_eq!(old.read(cx).model.jobs.rows[0].state, "running");
        });
        frame(cx, handle);
        let focus = shell(cx, handle, |_, window, cx| {
            let focus = old.read(cx).focus_handle().unwrap();
            assert!(focus.is_focused(window));
            focus
        });
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_input("/validated");
        visual.simulate_keystrokes("enter");
        visual.run_until_parked();
        frame(cx, handle);
        let sent: Vec<_> = std::iter::from_fn(&mut first_lines).collect();
        assert!(
            sent.iter()
                .any(|line| { line.contains("\"id\":0") && line.contains("\"op\":\"validate\"") }),
            "same-owner reuse keeps the real session observer and backend command route"
        );
        assert!(
            !std::iter::from_fn(&mut second_lines).any(|line| line.contains("\"op\":\"validate\""))
        );
        shell(cx, handle, |shell, window, cx| {
            assert!(matches!(
                old.read(cx).model.dialog,
                Some(Dialog::CopyTo { busy: true, .. })
            ));
            shell.receive(side, death(failed), window, cx);
        });
        frame(cx, handle);
        shell(cx, handle, |shell, window, cx| {
            assert!(
                shell.overlay.is_none(),
                "the current owner's backend death must close its mount"
            );
            assert_eq!(shell.operation_view(side).as_ref(), Some(&old));
            assert!(old.read(cx).model.dialog.is_none());
            assert_eq!(old.read(cx).model.jobs.rows[0].state, "failed");
            assert!(!focus.is_focused(window));
            assert!(
                shell.focus.is_focused(window),
                "current-owner cleanup restores listing focus"
            );
        });
        assert!(marks(cx, handle, "op1-scrim").is_none());
    }
}

#[path = "overlay_submit_tests.rs"]
mod submit;
