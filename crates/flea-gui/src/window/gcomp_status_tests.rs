use super::*;

#[gpui::test]
fn gcomp_mounted_disk_name_survives_pending_and_stale_producer_answers(cx: &mut TestAppContext) {
    let handle = open(cx);
    let answer = |cx: &mut TestAppContext, path: &str, name: &str| {
        edit(cx, handle, |shell, window, cx| {
            shell.receive(
                0,
                BackendEvent::Filesystem(crate::backend::status::Filesystem {
                    class: String::new(),
                    tagable: None,
                    path: path.into(),
                    name: name.into(),
                    free: Some(442_000_000_000),
                }),
                window,
                cx,
            )
        });
        frame(cx, handle);
    };
    answer(cx, "/files", "btrfs");
    edit(cx, handle, |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        assert!(pane.begin("/next".into(), Default::default(), pane.now()));
        for reply in [
            flea::protocol::Reply::Listed {
                path: "/next".into(),
                total: 0,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                rows: vec![],
                listing: 4,
            },
        ] {
            shell.receive(0, BackendEvent::Reply(reply), window, cx);
        }
    });
    frame(cx, handle);
    let check = |cx: &mut TestAppContext, expected: &str| {
        let state = json(cx, handle, "statusFooterState");
        assert_eq!(
            state.get("filesystem").and_then(Json::as_str),
            Some(expected)
        );
        edit(cx, handle, |shell, _, cx| {
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            assert!(
                shell.slots.status.is_some(),
                "the real producer was not mounted"
            );
            let drawn = footer.painted.get("disk").expect("a drawn disk zone");
            assert_eq!(drawn.text.as_ref(), expected);
            assert!(f32::from(drawn.bounds.size.width) > 0.);
            assert!(f32::from(drawn.bounds.size.height) > 0.);
            assert_eq!(
                footer.painted.get("counts").unwrap().text.as_ref(),
                "0 items"
            );
        });
    };
    check(cx, "btrfs · 442.0 GB free");
    answer(cx, "/files", "stale");
    check(cx, "btrfs · 442.0 GB free");
    answer(cx, "/next", "ext4");
    check(cx, "ext4 · 442.0 GB free");
}

#[gpui::test]
fn gcomp_owner_drain_releases_modal_focus_and_refusal_escape_restores_listing(
    cx: &mut TestAppContext,
) {
    let handle = open(cx);
    edit(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx)
    });
    frame(cx, handle);
    edit(cx, handle, |shell, window, cx| {
        assert!(!shell.focus.is_focused(window));
        shell.receive(1, BackendEvent::Drained, window, cx);
        assert!(shell.overlay.is_some(), "the other pane retired this card");
        assert!(!shell.focus.is_focused(window));
        shell.receive(0, BackendEvent::Drained, window, cx);
        assert!(shell.overlay.is_none());
        assert!(
            shell.focus.is_focused(window),
            "the retired card kept keyboard focus"
        );
    });
    let check_error = |cx: &mut TestAppContext, expected: &str| {
        assert_eq!(call(cx, handle, "statusError"), Reply::value("true"));
        assert_eq!(call(cx, handle, "lastMessage"), Reply::value(expected));
        edit(cx, handle, |shell, _, cx| {
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            let line = footer.line.read(cx);
            assert_eq!(line.messages().errors().len(), 1);
            assert_eq!(line.painted.get("primary").unwrap().text.as_ref(), expected);
        });
    };
    let dismiss_error = |cx: &mut TestAppContext| {
        edit(cx, handle, |shell, window, cx| {
            shell.key(
                &gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse("escape").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            );
            assert!(shell.focus.is_focused(window));
        });
        frame(cx, handle);
        assert_eq!(call(cx, handle, "statusError"), Reply::value("false"));
        edit(cx, handle, |shell, _, cx| {
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            assert!(footer.line.read(cx).messages().errors().is_empty());
        });
    };
    frame(cx, handle);
    check_error(cx, "The backend stopped accepting requests");
    dismiss_error(cx);
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            BackendEvent::Refused {
                operation: "window".into(),
                path: "/files".into(),
                message: "permission denied".into(),
                mode: 0,
            },
            window,
            cx,
        );
    });
    frame(cx, handle);
    check_error(cx, "Permission denied.");
    dismiss_error(cx);
}

#[gpui::test]
fn gcomp_u05_r3_empty_old_answer_stays_pending_until_the_new_directory_answers(
    cx: &mut TestAppContext,
) {
    let handle = open(cx);
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            BackendEvent::Filesystem(crate::backend::status::Filesystem {
                class: String::new(),
                tagable: None,
                path: "/files".into(),
                name: String::new(),
                free: Some(0),
            }),
            window,
            cx,
        );
        assert!(shell.surfaces[0].filesystem.answered);
    });
    frame(cx, handle);
    let unknown = crate::test_catalogs::english().tr("status.host.unknown");
    assert_eq!(
        json(cx, handle, "statusFooterState")
            .get("filesystem")
            .and_then(Json::as_str),
        Some(unknown.as_str())
    );
    edit(cx, handle, |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        assert!(pane.begin("/next".into(), Default::default(), pane.now()));
        for reply in [
            flea::protocol::Reply::Listed {
                path: "/next".into(),
                total: 0,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                rows: vec![],
                listing: 4,
            },
        ] {
            shell.receive(0, BackendEvent::Reply(reply), window, cx);
        }
        let fs = &shell.surfaces[0].filesystem;
        assert!(
            !fs.answered,
            "committing the new directory did not reset answered state"
        );
        assert_eq!(fs.facts.as_ref().unwrap().path, "/files");
    });
    for obsolete in [false, true] {
        if obsolete {
            edit(cx, handle, |shell, window, cx| {
                shell.receive(
                    0,
                    BackendEvent::Filesystem(crate::backend::status::Filesystem {
                        class: String::new(),
                        tagable: None,
                        path: "/files".into(),
                        name: "obsolete".into(),
                        free: Some(7),
                    }),
                    window,
                    cx,
                )
            });
        }
        frame(cx, handle);
        let state = json(cx, handle, "statusFooterState");
        assert_eq!(state.get("path").and_then(Json::as_str), Some("/next"));
        assert_eq!(state.get("filesystem").and_then(Json::as_str), Some(""));
        edit(cx, handle, |shell, _, cx| {
            let fs = &shell.surfaces[0].filesystem;
            assert!(!fs.answered);
            assert_eq!(fs.facts.as_ref().unwrap().path, "/files");
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            let disk = footer.painted.get("disk").unwrap();
            assert!(disk.text.is_empty());
            assert!(
                f32::from(disk.bounds.size.height) > 0.,
                "the check must observe the drawn producer"
            );
        });
    }
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            BackendEvent::Filesystem(crate::backend::status::Filesystem {
                class: String::new(),
                tagable: None,
                path: "/next".into(),
                name: String::new(),
                free: Some(0),
            }),
            window,
            cx,
        )
    });
    frame(cx, handle);
    assert_eq!(
        json(cx, handle, "statusFooterState")
            .get("filesystem")
            .and_then(Json::as_str),
        Some(unknown.as_str())
    );
    edit(cx, handle, |shell, _, _| {
        assert!(shell.surfaces[0].filesystem.answered)
    });
}
