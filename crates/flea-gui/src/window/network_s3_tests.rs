//! S3 network success retires its earlier refusal and returns a saved-card open to the list.
use super::*;
use crate::window::u07_tests::read;

#[gpui::test]
fn s3_ey580_identical_other_pane_refusal_survives_until_both_connect(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let origins = [shell.network_origin(0), shell.network_origin(1)];
            for (side, origin) in origins.into_iter().enumerate() {
                shell.focus_pane(side, cx);
                shell.network_service(
                    &Effect::Retry {
                        uri: "smb://missing/share".into(),
                        label: "Share".into(),
                        origin,
                        error: "network.refused",
                        args: vec![],
                        connect_failed: true,
                        password: Secret::new(),
                    },
                    window,
                    cx,
                );
            }
            let success = |origin| Effect::Shares {
                uri: "smb://live/".into(),
                label: "Live".into(),
                names: vec!["share".into()],
                origin,
            };
            shell.network_service(&success(origins[0]), window, cx);
            shell.status_messages(cx, |messages, _| {
                assert!(
                    messages.shown().is_error,
                    "pane 1's identical refusal still stands"
                );
            });
            shell.network_service(&success(origins[1]), window, cx);
            shell.status_messages(cx, |messages, _| {
                assert!(!messages.shown().is_error, "both panes have now connected");
            });
        })
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn s3_ey580_mount_success_clears_only_its_panes_connection_errors(cx: &mut TestAppContext) {
    for file in [false, true] {
        let recorder = Rc::new(Recorder::default());
        let (shell, cx, _lines) = open(recorder, cx);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let origin = shell.network_origin(0);
                for key in ["network.refused", "network.no_folder"] {
                    shell.network_service(
                        &Effect::Retry {
                            uri: "smb://missing/share".into(),
                            label: "Share".into(),
                            origin,
                            error: key,
                            args: vec![],
                            connect_failed: true,
                            password: Secret::new(),
                        },
                        window,
                        cx,
                    );
                }
                shell.status_refused("list", "permission denied", "", cx);
                let other = shell.network_origin(1);
                shell.network_service(
                    &Effect::Shares {
                        uri: "smb://other/".into(),
                        label: "Other".into(),
                        names: vec!["share".into()],
                        origin: other,
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    shell.status_messages(cx, |messages, _| messages.errors().len()),
                    3,
                    "a successful connect in the other pane keeps these errors"
                );
                let effect = if file {
                    Effect::OpenFile {
                        path: "/fixture/mounted/file.txt".into(),
                        uri: "smb://live/share/file.txt".into(),
                        label: "Share".into(),
                        origin,
                    }
                } else {
                    Effect::Open {
                        path: "/fixture/mounted".into(),
                        uri: "smb://live/share".into(),
                        label: "Share".into(),
                        origin,
                    }
                };
                shell.network_service(&effect, window, cx);
                shell.status_messages(cx, |messages, _| {
                    assert_eq!(
                        messages.errors().len(),
                        1,
                        "only the unrelated listing failure remains"
                    );
                });
            })
        });
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn s3_ey580_successful_share_browse_clears_the_earlier_refusal(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = open(Rc::new(Recorder::default()), cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let origin = shell.network_origin(0);
            shell.network_service(
                &Effect::Retry {
                    uri: "smb://".into(),
                    label: String::new(),
                    origin,
                    error: "network.refused",
                    args: vec![],
                    connect_failed: true,
                    password: Secret::new(),
                },
                window,
                cx,
            );
        })
    });
    frame(cx);
    assert_eq!(read(&shell, cx, "statusError", &[]), "true");
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert!(
        !read(&shell, cx, "statusPrimary", &[]).is_empty(),
        "closing the card keeps its error"
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let origin = shell.network_origin(0);
            shell.network_service(
                &Effect::Shares {
                    uri: "smb://127.0.0.1".into(),
                    label: "127.0.0.1".into(),
                    names: vec!["flea".into(), "secret".into()],
                    origin,
                },
                window,
                cx,
            );
        })
    });
    frame(cx);
    assert_eq!(read(&shell, cx, "statusPrimary", &[]), "");
    assert_eq!(read(&shell, cx, "lastMessage", &[]), "");
    assert_eq!(read(&shell, cx, "statusError", &[]), "false");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn s3_ey512_escape_after_the_saved_smb_card_connects_is_silent(cx: &mut TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let (shell, cx, _lines) = open_focus_tests::root(recorder.clone(), cx);
    crate::window::dispatch::test_shell::land(&shell, cx, "/fixture/work", Vec::new(), 1);
    handoff_tests::focus_rail(&shell, cx);
    shell.update(cx, |shell, cx| {
        shell.network_publish(
            Some(vec![RailEntry {
                uri: "smb://127.0.0.1/flea".into(),
                label: "Fixture share".into(),
                ..share()
            }]),
            cx,
        )
    });
    let sandbox = sandbox_bookmarks(&shell, cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let producer = shell.rail.as_ref().unwrap().places.clone();
            let entries = producer.read(cx).entries();
            let index = entries
                .iter()
                .position(|row| row.group == Group::Network)
                .unwrap();
            producer.update(cx, |producer, cx| producer.set_cursor(index, cx));
            shell.network_menu(0, &entries[index], window, cx);
        })
    });
    frame(cx);
    let rows = read(&shell, cx, "contextMenuEntries", &[]);
    let edit = rows
        .split('|')
        .position(|row| row == "Edit Address…")
        .unwrap_or_else(|| panic!("the saved share offers Edit Address: {rows:?}"))
        .to_string();
    for _ in 0..8 {
        if read(&shell, cx, "contextMenuCursor", &[]) == edit {
            break;
        }
        cx.simulate_keystrokes("down");
        frame(cx);
    }
    cx.simulate_keystrokes("enter");
    frame(cx);
    assert_eq!(read(&shell, cx, "dialogOpen", &[]), "true");
    cx.simulate_keystrokes("enter");
    frame(cx);
    answer_mount(&recorder, "/fixture/share", cx);
    written_bookmarks(sandbox, cx);
    crate::window::dispatch::test_shell::land(
        &shell,
        cx,
        "/fixture/share",
        ["one.txt", "two.txt", "three.txt"]
            .map(|name| flea::protocol::RowFacts {
                name: name.into(),
                ..Default::default()
            })
            .to_vec(),
        2,
    );
    frame(cx);
    assert_eq!(read(&shell, cx, "dialogOpen", &[]), "false");
    shell.read_with(cx, |shell, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        assert_eq!(pane.path(), "/fixture/share");
        assert_eq!(pane.held.total, 3);
        assert!(
            !pane.busy(),
            "Escape is pressed in the landed share listing"
        );
    });
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert_eq!(
        read(&shell, cx, "statusPrimary", &[]),
        "",
        "Escape in the connected listing is silent"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.keyboard.context),
        flea::keys::Context::Listing
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
