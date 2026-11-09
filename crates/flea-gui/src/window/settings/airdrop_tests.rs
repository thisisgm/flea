//! AirDrop's page gets the window's facts every time Settings opens on Plugins, however Settings was closed.
use crate::{backend::Backend, g2::st1::Request, state::tests::shell_with};
use flea::ui_logic_st2::airdrop::Page;
use gpui::TestAppContext;

#[gpui::test]
fn settings_reopened_on_plugins_refreshes_airdrops_page(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    shell.update_in(cx, |shell, window, cx| {
        shell.open_settings(window, cx);
        let panel = shell.settings.panel.clone().expect("Settings is built");
        panel.update(cx, |view, _| view.panel.show_section("plugins"));
        shell.sync_settings(window, cx);
        let view = shell
            .settings
            .plugins
            .pane
            .clone()
            .expect("Plugins is showing");
        let stale = Page {
            shown_as: "stale".into(),
            ..Default::default()
        };
        view.update(cx, |view, cx| view.set_airdrop(stale, cx));
        shell.settings_request(Request::Close, window, cx);
        assert!(!shell.settings.opened);
        shell.open_settings(window, cx);
        shell.sync_settings(window, cx);
        let shown_as = view.read(cx).page().airdrop_facts().shown_as.clone();
        assert_ne!(
            shown_as, "stale",
            "reopening Settings on Plugins asked for the window's facts again"
        );
    });
}
