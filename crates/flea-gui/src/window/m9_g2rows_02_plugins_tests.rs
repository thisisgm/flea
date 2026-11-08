//! Search owns its keys and results even when the Plugins pane held focus.
use super::configured;
use crate::backend::Backend;
use gpui::TestAppContext;

#[gpui::test]
fn m9_g2rows_02_search_from_plugins_owns_the_keyboard_and_pane(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = configured(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_keystrokes(", tab down down enter");
    shell.update_in(cx, |shell, window, cx| {
        shell.sync_settings(window, cx);
        assert!(shell.settings.plugins.pane.is_some());
        assert_eq!(
            shell
                .settings
                .panel
                .as_ref()
                .unwrap()
                .read(cx)
                .panel
                .section,
            "plugins"
        );
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("/ t h u m b");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    shell.update_in(cx, |shell, window, cx| {
        let view = shell.settings.panel.as_ref().unwrap().read(cx);
        assert_eq!(view.panel.query.as_deref(), Some("thumb"));
        assert!(view.focus.is_focused(window));
        assert!(!view.probe.rect("row:preview.thumbUsb").is_empty());
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 0);
    });
    cx.simulate_keystrokes("escape");
    shell.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        assert_eq!(panel.query, None);
        assert_eq!(panel.section, "plugins");
        assert!(shell.settings.opened);
    });
}

#[gpui::test]
fn m9_g2rows_02_search_never_types_into_the_live_listing_filter(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = configured(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    while requests().is_some() {}
    cx.simulate_keystrokes("/ x");
    assert_eq!(
        requests().as_deref(),
        Some(r#"{"c":"filter","query":"x","id":1}"#),
        "positive control: listing text reaches the real backend request"
    );
    shell.update_in(cx, |shell, window, cx| shell.open_settings(window, cx));
    cx.simulate_keystrokes("/ t h u m b");
    shell.read_with(cx, |shell, cx| {
        assert_eq!(
            shell
                .settings
                .panel
                .as_ref()
                .unwrap()
                .read(cx)
                .panel
                .query
                .as_deref(),
            Some("thumb")
        );
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.held.total, 1000);
        assert!(host.filter.owner.typing());
        assert_eq!(host.filter.owner.query(), "x");
    });
    assert_eq!(
        requests(),
        None,
        "Settings text never asks the listing backend"
    );
    cx.simulate_keystrokes("escape escape y");
    assert_eq!(
        requests().as_deref(),
        Some(r#"{"c":"filter","query":"xy","id":2}"#),
        "the listing line still accepts text after Settings closes"
    );
}
