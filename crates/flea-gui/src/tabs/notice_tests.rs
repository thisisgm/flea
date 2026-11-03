use super::*;

#[gpui::test]
fn u25_count_notice_withdrawal_preserves_a_newer_status_and_the_other_side(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.with_tabs(|_, panes| {
                panes.visit(0, &mut |tabs, pane| tabs.select_at(pane, 8));
            });
            let newer = shell.locale.catalog.tr("navigation.save_failed");
            shell.panes[0].as_mut().unwrap().status = newer.clone().into();
            key(shell, "t", window, cx);
            assert_eq!(shell.panes[0].as_ref().unwrap().status.as_ref(), newer);
            assert!(shell.strip.notice[0].text.is_empty());
            shell.panes[1] = Some(crate::window::pane::PaneHost::new(
                "/other".into(),
                None,
                620.,
                &shell.locale,
            ));
            shell.with_tabs(|_, panes| {
                panes.visit(0, &mut |tabs, pane| tabs.select_at(pane, 8));
                panes.visit(1, &mut |tabs, pane| tabs.select_at(pane, 8));
                panes.visit(0, &mut |tabs, pane| tabs.close_at(pane, 0));
            });
            assert!(shell.strip.notice[0].text.is_empty());
            assert!(!shell.strip.notice[1].text.is_empty());
            assert_eq!(
                shell.panes[1].as_ref().unwrap().status.as_ref(),
                shell.strip.notice[1].text
            );
            assert_eq!(
                shell.panes[0].as_ref().unwrap().status.as_ref(),
                shell.panes[0].as_ref().unwrap().state_text.status
            );
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn u25_delayed_spawn_failure_retires_only_its_own_lift_and_is_visible(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            let now = shell.tab_now();
            let failed = shell.with_tabs(|bar, panes| {
                bar.tab_lift_began(panes, 0, now);
                assert!(bar.tab_lift_moved(panes, 0., true));
                bar.tear_off_at(now).unwrap().token
            });
            shell.with_tabs(|bar, panes| {
                bar.tab_lift_began(panes, 1, now + 1);
                assert!(bar.tab_lift_moved(panes, 0., true));
            });
            let newer = shell.strip.bar.out_token.clone();
            shell.tab_window_failed(0, &failed, window, cx);
            assert!(shell.strip.bar.out_active);
            assert_eq!(shell.strip.bar.out_token, newer);
            assert!(shell.strip.bar.lifts.iter().any(|lift| lift.token == newer));
            assert!(!shell
                .strip
                .bar
                .lifts
                .iter()
                .any(|lift| lift.token == failed));
            assert_eq!(shell.strip.tabs[0].count(), 2);
            assert_eq!(
                shell.panes[0].as_ref().unwrap().status.as_ref(),
                shell.locale.catalog.tr("tabs.not_received")
            );
            shell.strip.bar.cancel_out(now + 2);
            shell.tabs_changed(window, cx);
        })
    });
    cx.run_until_parked();
}
