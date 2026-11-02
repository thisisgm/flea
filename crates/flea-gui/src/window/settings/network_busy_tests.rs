use crate::{backend::Backend, g2::pl2::dialog, state::tests::shell_with};
use gpui::TestAppContext;

#[gpui::test]
fn uish_misc_network_save_refuses_while_a_favourites_write_is_busy(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_lines();
    let (writes, queue) = std::sync::mpsc::channel();
    let (_send, updates) = async_channel::unbounded();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.rail = Some(crate::window::rail::mount(
            shell.locale.catalog.clone(),
            &shell.launch.settings.data,
            0,
            cx,
        ));
        shell.ensure_network(window, cx);
        shell.settings.worker = Some(super::super::store::Worker { writes, updates });
        shell
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.add_favourite("/files/busy", window, cx);
            assert_eq!(shell.settings.favourite_writes.len(), 1);
            let card = shell.network().unwrap().card.clone();
            card.update(cx, |card, _| {
                card.model.opened = true;
                card.model.saving = true;
                card.model.request = 7;
            });
            shell.network_card(
                &dialog::Effect::Save {
                    id: 7,
                    uri: "smb://host/share/".into(),
                    label: "Personal".into(),
                    replace: None,
                },
                window,
                cx,
            );
            let model = &card.read(cx).model;
            assert!(!model.saving);
            assert!(model.opened);
            assert!(model.retry);
            assert_eq!(
                model.detail.to_string(),
                "A Favorites change is still being saved. Retry to save this location."
            );
            assert_eq!(
                shell.settings_message(cx).map(|line| line.to_string()),
                Some("A Favorites change is still being saved.".into())
            );
            assert!(shell.settings.network_favourite.is_none());
            assert_eq!(shell.settings.favourite_writes.len(), 1);
        })
    });
    let queued: Vec<_> = queue.try_iter().collect();
    assert_eq!(queued.len(), 1, "the refused save queues no second write");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
