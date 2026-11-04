use crate::{
    backend::Backend, palette::Theme, presentation::Fonts, state::tests::shell_with, state::Shell,
};
use flea::ipc::wire::{Reply, Request};
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn u01_file_row_height_reads_the_last_painted_frame(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| super::root::Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    let mut rows = Vec::new();
    // The font base moves the painted row (Metrics::for_typeface), so a constant reader cannot match both.
    for base in [12, 16] {
        handle
            .update(cx, |root, _, cx| {
                root.shell.update(cx, |shell, cx| {
                    let fonts = Fonts::from_sources(
                        &format!("[font]\nbase-size={base}"),
                        "",
                        shell.active_typeface().font.family.to_string(),
                    );
                    let list = &shell.panes[0].as_ref().unwrap().list;
                    list.borrow_mut()
                        .set_typeface(fonts.typeface(&flea::jsondoc::Json::Null));
                    cx.notify();
                });
            })
            .unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        handle
            .update(cx, |root, window, cx| {
                root.shell.update(cx, |shell, cx| {
                    let drawn = shell.panes[0].as_ref().unwrap().list.borrow().drawn;
                    assert!(drawn.frames > 0, "the root painted the list");
                    let reply = call(shell, "fileRowHeight", window, cx);
                    assert_eq!(reply, Reply::value(drawn.metrics.row.round().to_string()));
                    rows.push(drawn.metrics.row);
                });
            })
            .unwrap();
    }
    assert_ne!(rows[0], rows[1], "both sizes painted one row: {rows:?}");
}

// EY-451: Theme.qml fileRowHeight follows density even while Grid retains an older list paint.
#[gpui::test]
fn ey451_file_row_height_follows_a_density_change_the_list_has_not_painted(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| super::root::Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                let painted = shell.panes[0].as_ref().unwrap().list.borrow().drawn;
                assert!(painted.frames > 0, "the root painted the list");
                let list = &shell.panes[0].as_ref().unwrap().list;
                list.borrow_mut().density = crate::list::Density::Normal;
                let now = list.borrow().file_metrics().row;
                assert_ne!(now, painted.metrics.row, "normal density moves the row");
                assert_eq!(
                    call(shell, "fileRowHeight", window, cx),
                    Reply::value(now.round().to_string())
                );
            });
        })
        .unwrap();
}

fn call(
    shell: &mut Shell,
    function: &str,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Shell>,
) -> Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    shell.ipc(
        crate::ipc::server::Event::Call {
            request: Request::Call {
                target: "flea".into(),
                function: function.into(),
                arguments: vec![],
            },
            answer,
        },
        window,
        cx,
    );
    receive.try_recv().expect("the live reader answers").reply
}

#[gpui::test]
fn u01_native_readers_follow_live_font_theme_and_window(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let (theme, parsed) = Theme::from_text_observed("foreground='#123456'");
        cx.set_global(theme);
        cx.set_global(parsed);
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            assert_eq!(call(shell, "bodyPx", window, cx), Reply::value("14"));
            assert_eq!(
                call(shell, "metrics", window, cx),
                Reply::value("13 12 14 37")
            );
            assert_eq!(call(shell, "themeLoaded", window, cx), Reply::value("true"));
            assert_eq!(
                call(shell, "themeForeground", window, cx),
                Reply::value("#123456")
            );
            let size = window.viewport_size();
            assert_eq!(
                call(shell, "windowSize", window, cx),
                Reply::value(format!(
                    "{} {}",
                    f32::from(size.width).round(),
                    f32::from(size.height).round()
                ))
            );
            shell.panes[0]
                .as_ref()
                .unwrap()
                .list
                .borrow_mut()
                .typeface
                .body = gpui::px(16.);
            assert_eq!(call(shell, "bodyPx", window, cx), Reply::value("16"));
            cx.set_global(Theme::from_text("foreground='#654321'"));
            assert_eq!(
                call(shell, "themeForeground", window, cx),
                Reply::value("#654321")
            );
            assert!(requests().is_none(), "observation starts no backend work");
        });
    });
    window.simulate_resize(gpui::size(gpui::px(640.), gpui::px(480.)));
    window.update(|window, cx| {
        let (backend, _) = Backend::test_channel();
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            assert_eq!(
                call(shell, "windowSize", window, cx),
                Reply::value("640 480")
            );
        });
    });
}

#[gpui::test]
fn u01_native_readers_refuse_unavailable_producers_and_report_parse_failure(
    cx: &mut TestAppContext,
) {
    let (backend, _) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let (theme, parsed) = Theme::from_text_observed("foreground='invalid'");
        cx.set_global(theme);
        cx.set_global(parsed);
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            assert_eq!(
                call(shell, "themeLoaded", window, cx),
                Reply::value("false")
            );
            assert_eq!(call(shell, "fileRowHeight", window, cx), Reply::value("31"));
            assert_eq!(
                call(shell, "palette", window, cx),
                Reply::error(3, "FLEA_IPC_UNAVAILABLE flea.palette: theme not loaded")
            );
            assert_eq!(
                call(shell, "tokens", window, cx),
                Reply::error(3, "FLEA_IPC_UNAVAILABLE flea.tokens: theme not loaded")
            );
            assert_eq!(call(shell, "wheelLines", window, cx), Reply::value("3"));
            assert_eq!(
                call(shell, "glass", window, cx),
                Reply::error(3, "FLEA_IPC_UNAVAILABLE flea.glass: window not mounted")
            );
        });
    });
}

#[gpui::test]
fn uish_04_mounted_palette_follows_runtime_theme_without_producer_work(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| super::root::Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    let mut replies = Vec::new();
    let mut token_replies = Vec::new();
    for (base, corner, text) in [
        (
            12,
            0.,
            "background='#101315'\nforeground='#c1c497'\naccent='#e5a55b'",
        ),
        (
            16,
            9.,
            "background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'",
        ),
    ] {
        handle
            .update(cx, |root, _, cx| {
                let (theme, parsed) = Theme::from_text_observed(text);
                cx.set_global(theme);
                cx.set_global(parsed);
                root.shell.update(cx, |shell, _| {
                    shell.corner = corner;
                    shell.launch.settings.data =
                        flea::jsondoc::parse(r#"{"columnWidths":{"size":117}}"#).unwrap();
                    let fonts = Fonts::from_sources(
                        &format!("[font]\nbase-size={base}"),
                        "",
                        shell.active_typeface().font.family.to_string(),
                    );
                    let mut list = shell.panes[0].as_ref().unwrap().list.borrow_mut();
                    list.set_typeface(fonts.typeface(&flea::jsondoc::Json::Null));
                });
            })
            .unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        handle
            .update(cx, |root, window, cx| {
                root.shell.update(cx, |shell, cx| {
                    let list = shell.panes[0].as_ref().unwrap().list.clone();
                    let (frames, misses, reads, drawn) = {
                        let list = list.borrow();
                        (
                            list.drawn.frames,
                            list.rows.misses,
                            list.palette_reads,
                            list.drawn.palette.expect("the mounted list painted"),
                        )
                    };
                    let _ = requests();
                    let reply = call(shell, "palette", window, cx);
                    assert_eq!(reply.code, 0, "{reply:?}");
                    let roles: Vec<_> = reply.stdout.split_whitespace().collect();
                    assert_eq!(roles.len(), 10, "the QML palette shape");
                    for (index, expected) in [
                        (0, drawn.background),
                        (2, drawn.foreground),
                        (4, drawn.accent),
                        (6, drawn.symlink),
                        (7, drawn.executable),
                        (8, drawn.accent_frame),
                        (9, drawn.muted_frame),
                    ] {
                        assert_eq!(roles[index], crate::ipc::rows::colour(expected));
                    }
                    let tokens = call(shell, "tokens", window, cx);
                    assert_eq!(tokens.code, 0, "{tokens:?}");
                    let lines: Vec<_> = tokens.stdout.lines().collect();
                    assert_eq!(lines.len(), 34);
                    for wanted in [
                        format!("baseSize={base}"),
                        format!("cornerRadius={corner}"),
                        "columnSize=117".into(),
                    ] {
                        assert!(
                            lines.contains(&wanted.as_str()),
                            "missing {wanted}: {tokens:?}"
                        );
                    }
                    assert_eq!(call(shell, "wheelLines", window, cx), Reply::value("3"));
                    let list = list.borrow();
                    assert_eq!(
                        (list.drawn.frames, list.rows.misses, list.palette_reads),
                        (frames, misses, reads)
                    );
                    assert!(requests().is_none(), "observation starts no backend work");
                    replies.push(reply.stdout);
                    token_replies.push(tokens.stdout);
                });
            })
            .unwrap();
    }
    assert_ne!(replies[0], replies[1], "a live theme change must reach IPC");
    assert_ne!(
        token_replies[0], token_replies[1],
        "effective fonts and corners change the mounted tokens"
    );
}

#[gpui::test]
fn uish_04_wheel_readback_matches_applied_listing_scroll(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(600.)));
    cx.run_until_parked();
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    assert!(frame.max_scroll() > 480.);
    let at = gpui::point(
        frame.bounds.left() + gpui::px(60.),
        frame.bounds.top() + gpui::px(frame.row * 2.5),
    );
    cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
    let before = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.scroll_y
    });
    // One physical notch arrives as the platform's three normalized lines in both policy controls.
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: at,
        delta: gpui::ScrollDelta::Lines(gpui::point(0., -3.)),
        ..Default::default()
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let reply = call(shell, "wheelLines", window, cx);
            assert_eq!(reply.code, 0, "{reply:?}");
            let lines: f32 = reply.stdout.trim().parse().unwrap();
            let after = shell.panes[0].as_ref().unwrap().pane.scroll_y;
            // Theme.qml's oracle, scroll.notchPx 24 and scroll.multiplier 4, pinned here and not read from wheel.rs.
            const NOTCH_PX: f32 = 24.;
            const MULTIPLIER: f32 = 4.;
            assert_eq!(after - before, lines * NOTCH_PX * MULTIPLIER);
            println!("UISH_04 WHEEL lines={lines} offset={}", after - before);
        });
    });
}
