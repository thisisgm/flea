use super::*;
use crate::ipc::Adapter;

#[gpui::test]
fn u25_tab_strip_keeps_its_completed_frame_until_the_pane_lands(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| shell.update(cx, |shell, cx| key(shell, "t", window, cx)));
    cx.run_until_parked();
    let before = cx.update(|_, cx| {
        shell
            .read(cx)
            .strip
            .snapshot
            .as_ref()
            .unwrap()
            .frame
            .clone()
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_tab(0, "/next".into(), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.strip.snapshot.as_ref().unwrap().frame,
                before,
                "no optimistic strip or label swap over held rows"
            );
            assert!(
                shell.strip.bounds.get().is_some(),
                "no blank strip while the pane is held"
            );
            let home = shell.locale.catalog.tr("tabs.home");
            assert_eq!(
                shell
                    .tab_adapter(&home)
                    .call("tabCentre", &["2".into()])
                    .unwrap(),
                Reply::value(""),
                "a new tab has no invented centre before its completed frame"
            );
            assert_eq!(
                shell.tab_adapter(&home).call("tabLabels", &[]).unwrap(),
                Reply::value("files|files")
            );
            land(shell, "/next", &[("one", false)], 3, window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, _| {
            let frame = &shell.strip.snapshot.as_ref().unwrap().frame;
            assert_eq!(frame.current, 2);
            assert_eq!(
                frame
                    .labels
                    .iter()
                    .map(|label| label.as_ref())
                    .collect::<Vec<_>>(),
                ["files", "files", "next"]
            );
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.path(), "/next");
            assert_eq!(shell.geometry(window).tabs.height, 27.);
        })
    });
}

#[gpui::test]
fn u25_held_centres_follow_identity_and_resize_the_completed_frame(cx: &mut TestAppContext) {
    use gpui::{px, size};
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            key(shell, "t", window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_tab(0, "/next".into(), window, cx);
            shell.with_tabs(|_, panes| {
                panes.visit(0, &mut |tabs, pane| tabs.close_at(pane, 0));
            });
            shell.tabs_changed(window, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_resize(size(px(400.), px(700.)));
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow_mut()
                .typeface
                .caption = px(15.);
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, _| {
            let snapshot = shell.strip.snapshot.as_ref().unwrap();
            assert_eq!(snapshot.frame.labels.len(), 3, "retain completed labels");
            let caption = f32::from(
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .list
                    .borrow()
                    .typeface
                    .caption,
            );
            assert_eq!(shell.strip.labels.borrow().typeface.caption, px(caption));
            assert_eq!(
                snapshot.frame.tab_width,
                crate::tabs::bar::tab_width(shell.geometry(window).tabs.width, 3, caption)
            );
            let bounds = shell.strip.bounds.get().unwrap();
            let home = shell.locale.catalog.tr("tabs.home");
            let expected_x = f32::from(bounds.origin.x)
                + crate::tabs::bar::PAD_X
                + 2.5 * snapshot.frame.tab_width;
            let expected_y = f32::from(bounds.origin.y) + crate::tabs::bar::HEIGHT / 2.;
            assert_eq!(
                shell
                    .tab_adapter(&home)
                    .call("tabCentre", &["1".into()])
                    .unwrap(),
                Reply::value(format!(
                    "{} {}",
                    (expected_x + 0.5).floor() as i64,
                    (expected_y + 0.5).floor() as i64
                ))
            );
            assert_eq!(
                shell
                    .tab_adapter(&home)
                    .call("tabCentre", &["2".into()])
                    .unwrap(),
                Reply::value(""),
                "new identity is not in the held frame"
            );
        })
    });
}

#[gpui::test]
fn u25_press_identity_survives_removal_before_the_drag_threshold(cx: &mut TestAppContext) {
    use gpui::{point, px, Modifiers, MouseButton};
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            key(shell, "t", window, cx);
        })
    });
    cx.run_until_parked();
    let (start, identity) = cx.update(|_, cx| {
        let shell = shell.read(cx);
        let bounds = shell.strip.bounds.get().unwrap();
        let width = shell.strip.snapshot.as_ref().unwrap().frame.tab_width;
        (
            point(
                bounds.origin.x + px(crate::tabs::bar::PAD_X + 1.5 * width),
                bounds.origin.y + px(13.),
            ),
            shell.strip.tabs[0].tabs.as_ref().unwrap().items[1].identity,
        )
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.with_tabs(|_, panes| {
                panes.visit(0, &mut |tabs, pane| tabs.close_at(pane, 0));
            });
            shell.tabs_changed(window, cx);
        })
    });
    cx.simulate_mouse_move(
        start + point(px(6.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        let index = shell.strip.bar.drag_from.expect("real gesture started");
        assert_eq!(
            shell.strip.tabs[0].tabs.as_ref().unwrap().items[index].identity,
            identity
        );
    });
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
}

// EY-394: a tab whose listing stays held past the cap is drawn and aimable, so a tear-off can be refused on it.
#[gpui::test]
fn u25_a_tab_held_past_the_cap_draws_and_answers_its_centre(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_tab(0, "/held".into(), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            let home = shell.locale.catalog.tr("tabs.home");
            assert_eq!(shell.strip.tabs[0].count(), 2);
            assert!(shell.panes[0].as_ref().unwrap().pane.busy());
            assert_eq!(
                shell
                    .tab_adapter(&home)
                    .call("tabCentre", &["1".into()])
                    .unwrap(),
                Reply::value(""),
                "inside the cap the completed frame still holds"
            );
        })
    });
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            let now = host.pane.now() + crate::pane::swap::CAP_MS + 1;
            assert!(
                host.pane.tick(now),
                "the held listing falls back at the cap"
            );
            host.changed(&shell.locale);
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, _| {
            assert!(
                shell.panes[0].as_ref().unwrap().pane.busy(),
                "still loading"
            );
            let home = shell.locale.catalog.tr("tabs.home");
            let adapter = shell.tab_adapter(&home);
            // EY-517: the loading tab keeps the live path's label, the one the path bar still reads.
            assert_eq!(
                adapter.call("tabLabels", &[]).unwrap(),
                Reply::value("files|files")
            );
            let bounds = shell.strip.bounds.get().expect("the strip is drawn");
            let width = shell.strip.snapshot.as_ref().unwrap().frame.tab_width;
            let x = f32::from(bounds.origin.x) + crate::tabs::bar::PAD_X + 1.5 * width;
            let y = f32::from(bounds.origin.y) + crate::tabs::bar::HEIGHT / 2.;
            assert_eq!(
                adapter.call("tabCentre", &["1".into()]).unwrap(),
                Reply::value(format!(
                    "{} {}",
                    (x + 0.5).floor() as i64,
                    (y + 0.5).floor() as i64
                ))
            );
            assert_eq!(shell.geometry(window).tabs.height, 27.);
        })
    });
}

// EY-517: a loading tab is labelled by the live path the path bar reads, as v0.3.12 TabBar.qml titles it.
#[gpui::test]
fn fix_views_2_ey517_a_loading_tab_is_labelled_by_its_live_path(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| shell.update(cx, |shell, cx| key(shell, "t", window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_tab(0, "/next".into(), window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.swap.holding = false;
            assert!(host.pane.busy(), "the listing of /next has not landed");
            shell.sync_tabs(window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.path(), "/files", "the path bar still reads files");
            let frame = &shell.strip.snapshot.as_ref().unwrap().frame;
            assert_eq!(frame.current, 2);
            assert_eq!(
                frame
                    .labels
                    .iter()
                    .map(|label| label.as_ref())
                    .collect::<Vec<_>>(),
                ["files", "files", "files"],
                "the tab and the path bar agree"
            );
        })
    });
}
