use super::*;
use crate::tabs::catcher;
use crate::tabs::transfer::Rect;
use gpui::{
    layer_shell::{Anchor, KeyboardInteractivity, Layer},
    point, px, WindowBackgroundAppearance, WindowKind,
};

// The source window at the compositor origin, 900 by 500, with its strip band across the top.
fn source_window(shell: &mut crate::state::Shell) {
    shell.strip.geometry.rect = Some(Rect {
        x: 0.,
        y: 0.,
        width: 900.,
        height: 500.,
    });
    shell.strip.geometry.strip = Some(Rect {
        x: 14.,
        y: 0.,
        width: 886.,
        height: 27.,
    });
}

#[gpui::test]
fn u25_native_desktop_catcher_is_bottom_transparent_and_never_takes_keyboard(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        let display = cx.displays().first().unwrap().id();
        let options = catcher::options(display);
        assert!(!options.focus);
        assert_eq!(
            options.window_background,
            WindowBackgroundAppearance::Transparent
        );
        let WindowKind::LayerShell(layer) = options.kind else {
            panic!("native layer surface")
        };
        assert_eq!(layer.namespace, "flea-tab-tearoff");
        assert_eq!(layer.layer, Layer::Bottom);
        assert_eq!(layer.anchor, Anchor::all());
        assert_eq!(layer.exclusive_zone, Some(gpui::px(-1.)));
        assert_eq!(layer.keyboard_interactivity, KeyboardInteractivity::None);
    });
}

#[gpui::test]
fn u25_desktop_destination_requires_its_actual_window_and_captured_payload(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 0, now));
            assert!(shell.tab_native_payload(cx).is_some());
            let payload = shell.strip.bar.out_payload.clone();
            let desktop = cx
                .windows()
                .into_iter()
                .find(|handle| shell.strip.catchers.contains(*handle))
                .expect("native catcher mapped");
            source_window(shell);
            assert!(!shell
                .strip
                .catchers
                .accepts(window.window_handle(), &payload));
            assert!(!shell.strip.catchers.accepts(desktop, "invalid"));
            assert!(shell.strip.catchers.accepts(desktop, &payload));
            let outside = point(px(1200.), px(300.));
            assert!(shell.take_desktop_tab("invalid", outside).is_none());
            let tear = shell.take_desktop_tab(&payload, outside).unwrap();
            assert_eq!(tear.token, shell.strip.bar.out_token);
            assert_eq!(tear.path, "/files");
            assert_eq!(
                shell.strip.tabs[0].count(),
                2,
                "source waits for destination acknowledgment"
            );
            assert!(
                shell.take_desktop_tab(&payload, outside).is_none(),
                "one physical drop launches once"
            );
            shell.tabs_changed(window, cx);
            assert!(!shell.strip.catchers.contains(desktop));
            assert!(!shell.strip.catchers.accepts(desktop, &payload));
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn u25_cancelled_drag_retires_the_desktop_destination_without_tearing_off(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 0, now));
            assert!(shell.tab_native_payload(cx).is_some());
            let payload = shell.strip.bar.out_payload.clone();
            shell.strip.bar.cancel_out(now);
            shell.tabs_changed(window, cx);
            source_window(shell);
            assert!(shell
                .take_desktop_tab(&payload, point(px(1200.), px(300.)))
                .is_none());
            assert_eq!(shell.strip.tabs[0].count(), 2);
            assert!(!cx
                .windows()
                .into_iter()
                .any(|handle| shell.strip.catchers.contains(handle)));
        })
    });
    cx.run_until_parked();
}

// v0.3.12 ui/js/Tabs.js:592-603: outside the source window tears off, its strip returns the tab, its body cancels.
#[gpui::test]
fn u25_a_catcher_drop_is_judged_against_the_source_window(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            key(shell, "t", window, cx);
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 0, now));
            let held = shell.strip.tabs[0].tabs.as_ref().unwrap().items[0].identity;
            assert!(shell.tab_native_payload(cx).is_some());
            let payload = shell.strip.bar.out_payload.clone();
            source_window(shell);
            assert!(
                shell
                    .take_desktop_tab(&payload, point(px(400.), px(300.)))
                    .is_none(),
                "a drop on the source body cancels"
            );
            assert!(shell.strip.bar.out_active, "the drag still owns its ending");
            assert!(!shell.strip.bar.own_accepted);
            let width = shell.strip.bar.tab_width(3);
            assert!(shell
                .take_desktop_tab(&payload, point(px(14. + 2.9 * width), px(12.)))
                .is_none());
            assert!(
                shell.strip.bar.own_accepted,
                "a drop on its own strip returns and reorders"
            );
            let order = shell.strip.tabs[0].tabs.as_ref().unwrap().items[2].identity;
            assert_eq!(
                order, held,
                "the held tab moved to the slot it was dropped on"
            );
            assert_eq!(shell.strip.tabs[0].count(), 3);
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn u25_an_unknown_source_rectangle_never_tears_off(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            key(shell, "t", window, cx);
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 0, now));
            assert!(shell.tab_native_payload(cx).is_some());
            let payload = shell.strip.bar.out_payload.clone();
            assert!(shell
                .take_desktop_tab(&payload, point(px(5000.), px(5000.)))
                .is_none());
            assert_eq!(shell.strip.tabs[0].count(), 2);
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn u25_a_lift_asks_the_compositor_for_its_own_window_once(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    let asked = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = asked.clone();
    let line = format!(
        r#"[{{"pid":{pid},"at":[10,20],"size":[900,500],"mapped":true,"hidden":false}},{{"pid":1,"at":[0,0],"size":[5,5],"mapped":true,"hidden":false}}]"#,
        pid = std::process::id()
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.strip.clients = std::sync::Arc::new(move || {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                line.clone()
            });
            key(shell, "t", window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 0, now));
            shell.begin_tab_geometry(cx);
            shell.begin_tab_geometry(cx);
            assert!(
                shell.strip.geometry.rect.is_none(),
                "never read on the UI thread"
            );
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            shell.strip.geometry.rect,
            Some(Rect {
                x: 10.,
                y: 20.,
                width: 900.,
                height: 500.
            })
        );
        assert!(shell.strip.geometry.strip.unwrap().width > 0.);
    });
}

#[gpui::test]
fn u25_finished_and_cancelled_tab_drags_close_the_desktop_catchers(cx: &mut TestAppContext) {
    for cancelled in [false, true] {
        let (backend, _requests) = Backend::test_channel();
        let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                key(shell, "t", window, cx);
                let now = shell.tab_now();
                shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 0, now));
                assert!(shell.tab_native_payload(cx).is_some());
                let catchers: Vec<_> = cx
                    .windows()
                    .into_iter()
                    .filter(|handle| shell.strip.catchers.contains(*handle))
                    .collect();
                assert!(!catchers.is_empty());
                if cancelled {
                    shell.strip.bar.cancel_out(now);
                } else {
                    shell.strip.bar.out_finished(now);
                }
                shell.tabs_changed(window, cx);
                assert!(
                    catchers.iter().all(|handle| !cx.windows().contains(handle)),
                    "cancelled={cancelled}: the catchers close when the drag ends"
                );
            })
        });
        cx.run_until_parked();
    }
}

// SCN-PROOF-4: tabTearoffState reads one compositor client query per lift, and the catcher layer's namespace.
#[gpui::test]
fn fixreaders_tab_tearoff_state_counts_one_client_query_per_lift(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    let line = format!(
        r#"[{{"pid":{pid},"at":[10,20],"size":[900,500],"mapped":true,"hidden":false}}]"#,
        pid = std::process::id()
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.strip.clients = std::sync::Arc::new(move || line.clone());
            key(shell, "t", window, cx);
        })
    });
    cx.run_until_parked();
    let state = |shell: &gpui::Entity<crate::state::Shell>, cx: &mut gpui::VisualTestContext| {
        let (answer, receive) = std::sync::mpsc::sync_channel(1);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.ipc(
                    crate::ipc::server::Event::Call {
                        request: flea::ipc::wire::Request::Call {
                            target: "flea".into(),
                            function: "tabTearoffState".into(),
                            arguments: Vec::new(),
                        },
                        answer,
                    },
                    window,
                    cx,
                )
            })
        });
        flea::jsondoc::parse(receive.try_recv().unwrap().reply.stdout.trim()).unwrap()
    };
    let before = state(&shell, cx);
    assert_eq!(before.get("lifts").and_then(|v| v.as_f64()), Some(0.));
    assert_eq!(
        before.get("namespace").and_then(|v| v.as_str()),
        Some("flea-tab-tearoff")
    );
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            let now = shell.tab_now();
            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, 0, now));
            shell.begin_tab_geometry(cx);
            shell.begin_tab_geometry(cx);
        })
    });
    cx.run_until_parked();
    let after = state(&shell, cx);
    assert_eq!(after.get("lifts").and_then(|v| v.as_f64()), Some(1.));
    assert_eq!(
        after.get("clientQueries").and_then(|v| v.as_f64()),
        Some(1.)
    );
}
