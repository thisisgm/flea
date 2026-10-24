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
            for verb in ["palette", "tokens", "wheelLines"] {
                assert_eq!(
                    call(shell, verb, window, cx),
                    Reply::error(3, format!("FLEA_IPC_NOT_PORTED flea.{verb}"))
                );
            }
        });
    });
}
