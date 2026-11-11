//! Conditional provider behavior is retained even though the primary specimens show it installed.
use super::{
    dialog::{Dialog, Effect, Focus},
    provider::ProviderStatus,
};
use gpui::AppContext;

#[test]
fn pl2_provider_check_is_per_open_stale_safe_and_install_preserves_the_oracle_command() {
    let mut d = Dialog::default();
    assert!(d.provider_check.is_none());
    let effects = d.open();
    let id = d.provider_check.unwrap();
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::CheckDropbox(n) if *n == id)));
    assert!(d.ring().contains(&Focus::Dropbox));
    assert!(!d.provider_status(id + 1, true));
    d.close();
    d.open();
    assert!(!d.provider_status(id, true));
    let id = d.provider_check.unwrap();
    assert!(d.provider_status(id, true));
    assert!(!d.ring().contains(&Focus::Dropbox));
    assert!(d.install_dropbox().is_empty());
    d.close();
    d.open();
    let id = d.provider_check.unwrap();
    d.provider_status(id, false);
    d.password.push_str("fixture-password");
    let effects = d.install_dropbox();
    assert!(effects.iter().any(|e| matches!(e, Effect::InstallDropbox)));
    assert!(effects.iter().any(|e| matches!(e, Effect::Closed)));
    assert!(d.password.is_empty());
    assert_eq!(
        flea::ui_logic_network_shares::DROPBOX_INSTALL,
        [
            "omarchy-launch-terminal",
            "omarchy",
            "install",
            "service",
            "dropbox"
        ]
    );
}

#[gpui::test]
fn pl2_provider_footer_draws_both_states_without_protocol_or_typing_height_drift(
    cx: &mut gpui::TestAppContext,
) {
    let (_root, catalog) = super::input_tests::catalog();
    let view = cx.add_window(|_, cx| {
        super::view::NetworkView::new(catalog, super::input_tests::palette(), cx)
    });
    view.update(cx, |v, window, cx| v.open(window, cx)).unwrap();
    for installed in [false, true] {
        view.update(cx, |v, _, cx| {
            if let Some(id) = v.model.provider_check {
                v.provider_status(&ProviderStatus { id, installed }, cx);
            } else {
                v.model.dropbox_installed = installed;
                v.changed(cx);
            }
        })
        .unwrap();
        cx.update_window(view.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        view.update(cx, |v, window, _| {
            let drawn = v.drawn.borrow();
            let wanted = super::drawing::HEIGHT + if installed { 0. } else { 79. };
            assert_eq!(
                drawn.card.unwrap().h,
                wanted.min(f32::from(window.viewport_size().height) - 16.)
            );
            assert_eq!(
                drawn.hits.iter().any(|(focus, _)| *focus == Focus::Dropbox),
                !installed
            );
        })
        .unwrap();
    }
}
