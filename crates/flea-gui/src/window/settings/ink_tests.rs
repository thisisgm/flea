use crate::{backend::Backend, list::Palette, palette::Theme};
use gpui::{TestAppContext, TextStyle};

// Sample input: a dark canvas under a light surface, so the two lifted reds differ.
pub(crate) const SPLIT: &str =
    "background='#202020'\nforeground='#e0e0e0'\ndark_background='#f0f0f0'\ncolor1='#c04040'";

#[gpui::test]
fn the_mounted_plugins_pane_takes_the_card_surface_and_its_red(cx: &mut TestAppContext) {
    let theme = Theme::from_text(SPLIT);
    let palette = Palette::inherited(&TextStyle::default(), theme);
    assert_ne!(theme.surface, theme.background);
    assert_ne!(palette.error, palette.error_on_surface);
    cx.update(|cx| cx.set_global(theme));
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = crate::state::tests::shell_with(backend, cx);
        shell.launch.settings.data = flea::uischema::defaults();
        shell.open_settings(window, cx);
        shell
    });
    entity.update_in(cx, |shell, window, cx| {
        let panel = shell.settings.panel.clone().unwrap();
        panel.update(cx, |view, _| view.panel.show_section("plugins"));
        // Mount: window/settings/plugins.rs builds the pane's tokens.
        shell.sync_settings_plugins(window, cx);
        let tokens = shell
            .settings
            .plugins
            .pane
            .as_ref()
            .unwrap()
            .read(cx)
            .tokens();
        assert_eq!(tokens.surface, theme.surface, "mounted pane ground");
        assert_eq!(tokens.error, palette.error_on_surface, "mounted pane red");
        // Refresh: a second theme makes the tokens window/settings/mod.rs rebuilds differ from the mount's.
        let next = Theme::from_text(
            &SPLIT
                .replace("#f0f0f0", "#d8e8f8")
                .replace("#c04040", "#a03050"),
        );
        let next_palette = Palette::inherited(&TextStyle::default(), next);
        assert_ne!(next.surface, theme.surface);
        assert_ne!(next_palette.error_on_surface, palette.error_on_surface);
        cx.set_global(next);
        shell.apply_corner(8., window, cx);
        let tokens = shell
            .settings
            .plugins
            .pane
            .as_ref()
            .unwrap()
            .read(cx)
            .tokens();
        assert_eq!(tokens.surface, next.surface, "refreshed pane ground");
        assert_eq!(
            tokens.error, next_palette.error_on_surface,
            "refreshed pane red"
        );
    });
}
