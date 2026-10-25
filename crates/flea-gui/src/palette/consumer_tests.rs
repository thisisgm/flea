//! The production presentation consumer: a published snapshot reaches the painted row inks and their IPC readers.
use super::Theme;
use crate::{
    backend::Backend,
    ipc::{rows::colour, server::Event},
    list::{bench, pane::Held},
    presentation::{Fonts, Snapshot},
    state::{tests::shell_with, Shell},
};
use flea::ipc::wire::Request;
use gpui::{px, size, AppContext, TestAppContext, WindowHandle};
use std::rc::Rc;

fn open(cx: &mut TestAppContext) -> WindowHandle<Shell> {
    let (backend, _receive) = Backend::test_channel();
    cx.open_window(size(px(1100.), px(700.)), |_, cx| {
        let mut shell = shell_with(backend, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.backend = None;
        let mut rows = bench::fixture("uniform");
        rows[1].mode = 0o120777;
        rows[1].link = Some("../elsewhere".into());
        rows[2].mode = 0o100755;
        rows[2].dir = false;
        host.pane.held = Rc::new(Held {
            listing: 2,
            total: rows.len(),
            start: 0,
            rows,
        });
        host.pane.cursor = 0;
        shell
    })
}

fn frame(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn ask(cx: &mut TestAppContext, handle: WindowHandle<Shell>, function: &str, row: &str) -> String {
    handle
        .update(cx, |shell, window, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: function.into(),
                        arguments: vec![row.into()],
                    },
                    answer,
                },
                window,
                cx,
            );
            let reply = receive.try_recv().unwrap().reply;
            assert_eq!(reply.code, 0, "{function}: {}", reply.stderr);
            reply.stdout.trim_end_matches('\n').to_owned()
        })
        .unwrap()
}

fn publish(cx: &mut TestAppContext, handle: WindowHandle<Shell>, colors: &str) -> Theme {
    let (theme, parsed) = Theme::from_text_observed(colors);
    let snapshot = Snapshot {
        theme,
        parsed,
        fonts: Fonts::from_sources("", "", "JetBrains Mono".into()),
    };
    handle
        .update(cx, |shell, window, cx| {
            shell.apply_presentation(snapshot, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    theme
}

#[test]
fn m9_integ_g05_a_published_theme_paints_link_and_executable_rows_in_their_roles() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    let mut seen = Vec::new();
    for colors in [
        "background='#101315'\nforeground='#cacccc'\naccent='#e5a55b'\ncyan='#56b6c2'\ngreen='#98c379'",
        "background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ncyan='#04a5e5'\ngreen='#40a02b'",
    ] {
        let theme = publish(&mut cx, handle, colors);
        let link = ask(&mut cx, handle, "rowNameColor", "1");
        let program = ask(&mut cx, handle, "rowNameColor", "2");
        assert_eq!(link, colour(theme.symlink), "the link row lost the symlink role");
        assert_eq!(program, colour(theme.executable), "the program row lost the executable role");
        assert_ne!(link, colour(theme.foreground));
        assert_ne!(program, colour(theme.foreground));
        let painted = handle
            .update(&mut cx, |shell, _, _| {
                shell.panes[0].as_ref().unwrap().list.borrow().drawn.palette.unwrap()
            })
            .unwrap();
        assert_eq!(painted.metadata, theme.metadata, "the frame painted a stale metadata role");
        seen.push(link);
    }
    assert_ne!(
        seen[0], seen[1],
        "a second published theme did not repaint the link ink"
    );
}

#[test]
fn inherited_red_keeps_code_palette_and_gutter_roles() {
    let colors = "background='#1e1e2e'\ndark_background='#161622'\nforeground='#cdd6f4'";
    let themes =
        ["#f38ba8", "#f7768e"].map(|red| Theme::from_text(&format!("{colors}\nred='{red}'")));
    assert_ne!(themes[0].code_inks[0], themes[1].code_inks[0]);
    assert_eq!(themes[0].code_inks[1..], themes[1].code_inks[1..]);
    assert_eq!(themes[0].gutter, themes[1].gutter);
    let errors = themes.map(|theme| {
        let style = gpui::TextStyle {
            color: theme.foreground,
            background_color: Some(theme.surface),
            ..Default::default()
        };
        crate::list::Palette::inherited(&style, theme).error
    });
    assert_ne!(
        errors[0], errors[1],
        "inherited error ink must follow the theme red"
    );
}
