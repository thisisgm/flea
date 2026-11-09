use super::*;
use crate::{backend::Backend, g2::network_shares::dialog, state::tests::shell_with};
use gpui::TestAppContext;

#[gpui::test]
fn uish_misc_favourite_committed_response_never_replays_the_write(cx: &mut TestAppContext) {
    use super::super::store::{Update, Worker};
    use std::time::{Duration, Instant};
    let document = r#"{"places":{"favourites":[{"label":"Personal","path":"smb://host/share/"}]}}"#;
    for response in ["not-json", "{}", "[]", r#"{"places":{"favourites":17}}"#] {
        let root = crate::g2::device_rail::bfx_net_tests::Root::new();
        let program = root.script(
            "writer",
            &format!("#!/bin/sh\nwriter_root=$(dirname \"$0\")\nprintf 'committed\\n' >> \"$writer_root/commits\"\nprintf '%s\\n' '{document}' > \"$writer_root/ui.json\"\nprintf '%s\\n' '{response}'\n"),
        );
        let worker = Worker::at(root.0.join("ui.json"), program.into()).unwrap();
        let updates = worker.updates.clone();
        let (backend, _) = Backend::test_lines();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let mut shell = shell_with(backend, cx);
            shell.rail = Some(crate::window::rail::mount(
                shell.locale.catalog.clone(),
                &shell.launch.settings.data,
                0,
                cx,
            ));
            shell.ensure_network(window, cx);
            shell.settings.worker = Some(worker);
            shell
        });
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.network().unwrap().card.update(cx, |card, _| {
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
            })
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(update) = updates.try_recv() {
                if !matches!(
                    update,
                    Update::Corner(_) | Update::Unreadable | Update::Loaded { through: 0, .. }
                ) {
                    cx.update(|window, cx| {
                        shell.update(cx, |shell, cx| shell.receive_settings(update, window, cx))
                    });
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "missing committed acknowledgement"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        shell.update(cx, |shell, cx| {
            let notice = shell.locale.catalog.tr("places.favorite.saved_unread");
            let card = shell.network().unwrap().card.clone();
            card.update(cx, |card, _| {
                assert!(
                    card.model.opened && card.model.committed && !card.model.saving,
                    "{response}"
                );
                assert_eq!(card.model.status, Some("network.save_committed"));
                assert_eq!(card.model.detail, notice);
                assert!(matches!(
                    card.model.submit().as_slice(),
                    [dialog::Effect::Closed]
                ));
            });
            assert!(shell.settings.network_favourite.is_none());
            assert!(
                shell.settings.pending.is_empty() && shell.settings.favourite_writes.is_empty()
            );
            shell.stop_for_app_quit();
        });
        assert_eq!(
            std::fs::read_to_string(root.0.join("commits")).unwrap(),
            "committed\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.0.join("ui.json"))
                .unwrap()
                .trim(),
            document
        );
    }
}

#[gpui::test]
fn uish_misc_native8_network_save_keeps_the_real_writer_refusal(cx: &mut TestAppContext) {
    use super::super::store::{Update, Worker};
    use std::time::{Duration, Instant};
    let root = crate::g2::device_rail::bfx_net_tests::Root::new();
    let program = root.script(
        "writer",
        "#!/bin/sh\nprintf '%s\\n' 'flea: fixture/ui.json could not be read, so no settings were written' 'second diagnostic' >&2\nexit 2\n",
    );
    let worker = Worker::at(root.0.join("ui.json"), program.into()).unwrap();
    let updates = worker.updates.clone();
    let (backend, _) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.rail = Some(crate::window::rail::mount(
            shell.locale.catalog.clone(),
            &shell.launch.settings.data,
            0,
            cx,
        ));
        shell.ensure_network(window, cx);
        shell.settings.worker = Some(worker);
        shell
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.network().unwrap().card.update(cx, |card, _| {
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
        })
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(update) = updates.try_recv() {
            if !matches!(
                update,
                Update::Corner(_) | Update::Unreadable | Update::Loaded { through: 0, .. }
            ) {
                cx.update(|window, cx| {
                    shell.update(cx, |shell, cx| shell.receive_settings(update, window, cx))
                });
                break;
            }
        }
        assert!(Instant::now() < deadline, "missing writer acknowledgement");
        std::thread::sleep(Duration::from_millis(5));
    }
    shell.update(cx, |shell, cx| {
        let card = shell.network().unwrap().card.read(cx);
        assert!(card.model.opened && card.model.retry && !card.model.saving);
        assert_eq!(
            card.model.detail,
            "fixture/ui.json could not be read, so no settings were written"
        );
        assert!(shell.settings.network_favourite.is_none());
        shell.stop_for_app_quit();
    });
}

#[test]
fn uish_misc_settings_writer_reports_each_queued_revision_in_order() {
    use super::super::store::{Update, Worker};
    use std::time::{Duration, Instant};
    let root = crate::g2::device_rail::bfx_net_tests::Root::new();
    let program = root.script(
        "writer",
        "#!/bin/sh\nprintf '%s\\n' '{\"places\":{\"favourites\":[]}}'\n",
    );
    let worker = Worker::at(root.0.join("ui.json"), program.into()).unwrap();
    let empty = Json::Obj(Vec::new());
    worker
        .writes
        .send((7, empty.clone(), Some(empty.clone())))
        .unwrap();
    worker.writes.send((8, empty, None)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut observed = Vec::new();
    while observed.len() < 2 {
        match worker.updates.try_recv() {
            Ok(Update::Loaded { through, saved, .. }) if through != 0 => {
                observed.push((through, saved));
            }
            Ok(Update::Failed { .. }) => panic!("the fixture writer failed"),
            _ => {}
        }
        assert!(
            Instant::now() < deadline,
            "missing per-request acknowledgement"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(observed, [(7, false), (8, true)]);
}

#[gpui::test]
fn uish_misc_network_save_waits_for_the_locked_favourite_writer(cx: &mut TestAppContext) {
    for outcome in ["saved", "failed", "closed"] {
        let (backend, _) = Backend::test_lines();
        let (writes, queue) = std::sync::mpsc::channel();
        let mut queued = Some(queue);
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
        if outcome == "closed" {
            queued.take();
        }
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let card = shell.network().unwrap().card.clone();
                card.update(cx, |card, _| {
                    card.model.opened = true;
                    card.model.saving = true;
                    card.model.request = 7;
                });
                let before = shell.launch.settings.data.clone();
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
                assert_eq!(
                    shell.launch.settings.data, before,
                    "no optimistic Favorite before acknowledgement"
                );
                if outcome == "closed" {
                    assert!(!card.read(cx).model.saving);
                    assert!(card.read(cx).model.opened);
                    assert!(!card.read(cx).model.detail.is_empty());
                    assert!(shell.settings.network_favourite.is_none());
                }
            })
        });
        if outcome != "closed" {
            let (revision, _, operation) = queued
                .take()
                .unwrap()
                .try_recv()
                .expect("the Favorites operation writer must own the save");
            let operation = operation.unwrap();
            assert_eq!(operation.get("op").and_then(Json::as_str), Some("add"));
            assert_eq!(
                operation
                    .get("record")
                    .unwrap()
                    .get("path")
                    .and_then(Json::as_str),
                Some("smb://host/share/")
            );
            cx.update(|window, cx| {
                shell.update(cx, |shell, cx| {
                    let card = shell.network().unwrap().card.clone();
                    assert!(card.read(cx).model.saving);
                    let before = shell.launch.settings.data.clone();
                    shell.receive_settings(
                        super::super::store::Update::Loaded {
                            document: before.clone(),
                            through: revision - 1,
                            saved: false,
                        },
                        window,
                        cx,
                    );
                    assert!(
                        card.read(cx).model.saving,
                        "an older settings read cannot acknowledge this save"
                    );
                    let update = if outcome == "saved" {
                        super::super::store::Update::Loaded {
                            document: flea::favourites::changed(&before, &operation).unwrap(),
                            through: revision,
                            saved: false,
                        }
                    } else {
                        super::super::store::Update::Failed {
                            through: revision,
                            keep: false,
                        }
                    };
                    shell.receive_settings(update, window, cx);
                    assert!(!card.read(cx).model.saving);
                    assert_eq!(card.read(cx).model.opened, outcome != "saved");
                    assert!(shell.settings.network_favourite.is_none());
                    let favourites = shell
                        .launch
                        .settings
                        .data
                        .get("places")
                        .unwrap()
                        .get("favourites")
                        .unwrap()
                        .as_array()
                        .unwrap();
                    assert_eq!(favourites.len(), usize::from(outcome == "saved"));
                    if outcome != "saved" {
                        assert!(!card.read(cx).model.detail.is_empty());
                    }
                })
            });
        }
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}
