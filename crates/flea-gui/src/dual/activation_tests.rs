//! A listed line alone cannot expose an unfinished secondary surface.
use crate::{
    backend::{Backend, Event},
    state::tests::shell_with,
    window::{pane::PaneHost, startup::View},
};
use flea::protocol::{Reply, RowFacts};
use gpui::{AppContext, Render, TestAppContext};
use std::rc::Rc;

#[gpui::test]
fn u26_dual_activation_presents_the_secondary_only_with_a_complete_listing(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            let original = Rc::clone(&shell.panes[0].as_ref().unwrap().pane.held);
            shell.panes[1] = Some(PaneHost::new("/right".into(), None, 620., &shell.locale));
            shell.set_view(View::Dual, window, cx);
            assert_eq!(
                shell.view,
                View::List,
                "the completed single layout must stand until the secondary is ready"
            );
            let _ = shell.render(window, cx);
            assert_eq!(
                shell.panes[1].as_ref().unwrap().list.borrow().rows.misses,
                0
            );
            shell.receive(
                1,
                Event::Reply(Reply::Listed {
                    path: "/right".into(),
                    total: 2,
                }),
                window,
                cx,
            );
            assert_eq!(
                shell.view,
                View::List,
                "listed without rows is not a completed surface"
            );
            shell.receive(
                1,
                Event::Reply(Reply::Rows {
                    start: 0,
                    rows: vec![RowFacts::default(); 2],
                    listing: 2,
                }),
                window,
                cx,
            );
            assert_eq!(shell.view, View::Dual);
            assert_eq!(shell.panes[1].as_ref().unwrap().pane.held.rows.len(), 2);
            assert!(Rc::ptr_eq(
                &original,
                &shell.panes[0].as_ref().unwrap().pane.held
            ));
        });
    });
}

#[gpui::test]
fn u26_cancelled_dual_activation_never_blinks_back_when_rows_arrive(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.panes[1] = Some(PaneHost::new("/right".into(), None, 620., &shell.locale));
            shell.set_view(View::Dual, window, cx);
            shell.set_view(View::Grid, window, cx);
            shell.receive(
                1,
                Event::Reply(Reply::Listed {
                    path: "/right".into(),
                    total: 0,
                }),
                window,
                cx,
            );
            shell.receive(
                1,
                Event::Reply(Reply::Rows {
                    start: 0,
                    rows: vec![],
                    listing: 2,
                }),
                window,
                cx,
            );
            assert_eq!(shell.view, View::Grid);
            assert!(!shell.dual_pending);
        });
    });
}

#[gpui::test]
fn u26_dual_activation_uses_the_existing_fallback_and_shows_a_stopped_secondary(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.panes[1] = Some(PaneHost::new("/right".into(), None, 620., &shell.locale));
            shell.set_view(View::Dual, window, cx);
            shell.panes[1]
                .as_mut()
                .unwrap()
                .pane
                .tick(crate::pane::swap::CAP_MS);
            shell.settle_dual(window, cx);
            assert_eq!(shell.view, View::Dual);
            assert!(shell.mark_drawn(1));
            shell.set_view(View::List, window, cx);
            shell.panes[1] = Some(PaneHost::new("/stopped".into(), None, 620., &shell.locale));
            shell.panes[1].as_mut().unwrap().stopped = true;
            shell.set_view(View::Dual, window, cx);
            assert_eq!(
                shell.view,
                View::Dual,
                "a stopped secondary activates on its error state, never a silent refusal"
            );
            assert!(!shell.dual_pending);
        });
    });
}

#[gpui::test]
fn rem_j2_dual_reentry_and_start_pane_keep_the_live_host(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.panes[1] = Some(PaneHost::new("/right".into(), None, 620., &shell.locale));
            let held = |shell: &crate::state::Shell, side: usize| {
                Rc::clone(&shell.panes[side].as_ref().unwrap().pane.held)
            };
            let (left, right) = (held(shell, 0), held(shell, 1));
            for view in [View::Dual, View::List, View::Dual] {
                shell.set_view(view, window, cx);
            }
            shell.start_pane(0, window, cx);
            shell.start_pane(1, window, cx);
            // A host that is never replaced never drops its Grid pictures outside drop_image.
            assert!(
                Rc::ptr_eq(&left, &held(shell, 0)),
                "start_pane replaced the live left host"
            );
            assert!(
                Rc::ptr_eq(&right, &held(shell, 1)),
                "dual re-entry replaced the live right host"
            );
        });
    });
}
