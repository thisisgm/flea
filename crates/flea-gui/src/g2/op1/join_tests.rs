//! The composed root passes size and theme changes through the existing overlay slot.
use crate::{backend::Backend, palette::Theme, state::tests::shell_with, window::root::Root};
use gpui::{px, size, AppContext, TestAppContext, VisualTestContext};

#[gpui::test]
fn fix9_composed_copy_to_keeps_live_geometry_and_focus(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let view = cx.add_window(|window, cx| {
        window.set_rem_size(px(14.));
        let shell = cx.new(|cx| shell_with(backend, cx));
        Root { shell }
    });
    cx.simulate_window_resize(view.into(), size(px(1100.), px(700.)));
    view.update(cx, |root, window, cx| {
        root.shell.update(cx, |shell, cx| {
            shell.open_overlay_copy_to(false, "/fx/v/work", window, cx)
        });
    })
    .unwrap();
    let shell = view.update(cx, |root, _, _| root.shell.clone()).unwrap();
    for (base, width) in [(12., 473.), (16., 630.), (14., 552.)] {
        view.update(cx, |root, window, cx| {
            assert_eq!(window.rem_size(), px(14.));
            root.shell.update(cx, |shell, cx| {
                let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
                let fonts = crate::presentation::Fonts::from_sources(
                    &format!("[font]\nbase-size={base}"),
                    "",
                    shell.active_typeface().font.family.to_string(),
                );
                shell.apply_presentation(
                    crate::presentation::Snapshot {
                        theme,
                        parsed: crate::palette::ParsedTheme(true),
                        fonts,
                    },
                    window,
                    cx,
                );
                shell
                    .overlay
                    .as_ref()
                    .unwrap()
                    .update(cx, |_, cx| cx.notify());
                cx.notify();
            });
            window.refresh();
        })
        .unwrap();
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(view.into(), cx);
        assert_eq!(
            visual.debug_bounds("op1-card").unwrap().size.width,
            px(width)
        );
        visual.update(|window, cx| {
            assert_eq!(window.rem_size(), px(14.));
            let shell = shell.read(cx);
            let active = shell.active_typeface();
            assert_eq!(active.base, px(base));
            assert_eq!(active.body, px(base));
            assert_eq!(crate::list::Typeface::for_window(window, cx), active);
            let overlay = shell.overlay.as_ref().unwrap().read(cx);
            let drawn = overlay.editor.drawn.borrow();
            assert_eq!(drawn.font.as_ref(), Some(&active.font));
            assert_eq!(drawn.pixels, active.body);
            assert_eq!(
                drawn.line.run.as_ref().unwrap().line().font_size,
                active.body
            );
            assert!(overlay.focus_handle().unwrap().is_focused(window));
        });
    }
    cx.update(|cx| cx.set_global(Theme::from_text("background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\ndark_background='#e6e9ef'")));
    view.update(cx, |root, window, cx| {
        root.shell
            .read(cx)
            .overlay
            .clone()
            .unwrap()
            .update(cx, |_, cx| cx.notify());
        window.refresh();
    })
    .unwrap();
    cx.run_until_parked();
    view.update(cx, |root, _, cx| {
        let shell = root.shell.read(cx);
        let palette = shell.overlay.as_ref().unwrap().read(cx).palette;
        assert_eq!(palette.accent, Theme::from_text("accent='#1e66f5'").accent);
        assert_eq!(
            palette.accent_frame,
            Theme::from_text("accent='#1e66f5'").accent
        );
        assert_eq!(
            palette.surface,
            Theme::from_text("dark_background='#e6e9ef'").surface
        );
    })
    .unwrap();
}
