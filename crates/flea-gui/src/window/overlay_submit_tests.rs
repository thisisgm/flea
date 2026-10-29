//! Mounted cross-owner submission retains the canonical per-pane backend route.
use super::{cancel_card, death, frame, marks, mounted_session, shell, Dialog, View};
use gpui::{TestAppContext, VisualTestContext};

fn cross_owner_submit(cx: &mut TestAppContext, dead: usize) {
    for failed in [true, false] {
        let (handle, mut first_lines, mut second_lines) = mounted_session(cx, dead);
        let old = shell(cx, handle, |shell, _, _| {
            shell.operation_view(dead).unwrap()
        });
        cancel_card(cx, handle);
        let _: Vec<_> = std::iter::from_fn(&mut first_lines).collect();
        let _: Vec<_> = std::iter::from_fn(&mut second_lines).collect();
        let owner = 1 - dead;
        shell(cx, handle, |shell, window, cx| {
            shell.focus_pane(owner, cx);
            assert_eq!(shell.view, View::Dual);
            assert_eq!(shell.focus_side, owner);
            assert!(shell.panes[owner].as_ref().unwrap().pane.held.listing > 0);
            shell.open_overlay_copy_to(false, "/destination", window, cx);
            assert!(shell
                .overlay
                .as_ref()
                .unwrap()
                .read(cx)
                .focus_handle()
                .unwrap()
                .is_focused(window));
        });
        frame(cx, handle);
        assert!(marks(cx, handle, "op1-scrim").is_some());
        let (live, focus) = shell(cx, handle, |shell, window, cx| {
            let live = shell.overlay.clone().unwrap();
            assert_eq!(live.read(cx).owner, Some(owner));
            assert_ne!(live, old);
            assert!(matches!(
                live.read(cx).model.dialog,
                Some(Dialog::CopyTo {
                    menu: 0,
                    busy: false,
                    ..
                })
            ));
            assert_eq!(old.read(cx).owner, Some(dead));
            assert!(old.read(cx).model.dialog.is_none());
            assert_eq!(old.read(cx).model.jobs.rows[0].state, "running");
            let focus = live.read(cx).focus_handle().unwrap();
            assert!(focus.is_focused(window));
            (live, focus)
        });
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        visual.simulate_input("/validated/cross-owner");
        visual.simulate_keystrokes("enter");
        visual.run_until_parked();
        frame(cx, handle);
        let first: Vec<_> = std::iter::from_fn(&mut first_lines).collect();
        let second: Vec<_> = std::iter::from_fn(&mut second_lines).collect();
        let (sent, foreign) = if owner == 0 {
            (first, second)
        } else {
            (second, first)
        };
        assert!(
            foreign.is_empty(),
            "cross-owner submit must never send to the old backend"
        );
        assert_eq!(
            sent,
            if owner == 1 {
                vec![r#"{"c":"fsinfo"}"#.to_owned(), r#"{"c":"menuaction","op":"validate","id":0,"action":"copyTo","dest":"/validated/cross-owner"}"#.to_owned()]
            } else {
                vec![r#"{"c":"menuaction","op":"validate","id":0,"action":"copyTo","dest":"/validated/cross-owner"}"#.to_owned()]
            },
            "cross-owner submit must preserve the exact focus and validate request queue"
        );
        let pending = shell(cx, handle, |shell, _, cx| {
            assert_eq!(shell.operation_view(owner).as_ref(), Some(&live));
            assert_eq!(shell.operation_view(dead).as_ref(), Some(&old));
            assert!(matches!(
                live.read(cx).model.dialog,
                Some(Dialog::CopyTo { busy: true, .. })
            ));
            assert_eq!(old.read(cx).model.jobs.rows[0].state, "running");
            live.read(cx).model.dialog.clone()
        });
        shell(cx, handle, |shell, window, cx| {
            shell.receive(dead, death(failed), window, cx)
        });
        frame(cx, handle);
        shell(cx, handle, |shell, window, cx| {
            assert_eq!(shell.overlay.as_ref(), Some(&live));
            assert_eq!(live.read(cx).owner, Some(owner));
            assert_eq!(live.read(cx).model.dialog, pending);
            assert!(focus.is_focused(window));
            assert_eq!(shell.operation_view(dead).as_ref(), Some(&old));
            assert_eq!(old.read(cx).owner, Some(dead));
            assert!(old.read(cx).model.dialog.is_none());
            assert_eq!(old.read(cx).model.jobs.rows[0].state, "failed");
            assert!(!old.read(cx).model.wire.transfer.running);
        });
        assert!(marks(cx, handle, "op1-scrim").is_some());
        let first_after_death: Vec<_> = std::iter::from_fn(&mut first_lines).collect();
        let second_after_death: Vec<_> = std::iter::from_fn(&mut second_lines).collect();
        for (side, queue) in [&first_after_death, &second_after_death]
            .into_iter()
            .enumerate()
        {
            if !failed && side == dead {
                assert!(queue.is_empty(),
                    "Drained backend must have no post-death commands: dead={dead} failed={failed} first={first_after_death:?} second={second_after_death:?}");
            } else {
                assert!(queue.is_empty() || (queue.len() == 1 && queue[0] == r#"{"c":"fsinfo"}"#),
                    "live backend permits only one exact background fsinfo: dead={dead} failed={failed} side={side} first={first_after_death:?} second={second_after_death:?}");
            }
        }
    }
}

#[gpui::test]
fn g70_cross_owner_submit_from_left(cx: &mut TestAppContext) {
    cross_owner_submit(cx, 0);
}

#[gpui::test]
fn g70_cross_owner_submit_from_right(cx: &mut TestAppContext) {
    cross_owner_submit(cx, 1);
}
