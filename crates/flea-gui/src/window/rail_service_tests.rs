//! U08 service failure and request ownership checks through the real window.
use super::*;

#[gpui::test]
fn u08_rename_commits_the_captured_share_and_rebuild_cancels_a_stale_editor(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(false, cx);
    let share = RailEntry {
        group: Group::Network,
        kind: Kind::Share,
        label: "NAS".into(),
        uri: "smb://nas/share".into(),
        glyph: "server",
        ..RailEntry::default()
    };
    rows(&shell, vec![share.clone()], cx);
    let heard = Rc::new(RefCell::new(Vec::<Request>::new()));
    let sink = heard.clone();
    cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, request: &Request, _| {
            sink.borrow_mut().push(request.clone())
        })
        .detach()
    });
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    cx.simulate_keystrokes("f2");
    frame(cx);
    assert_eq!(
        ipc(&shell, "railRenameEditorLive", &[], cx).stdout.trim(),
        "true"
    );
    assert_eq!(
        ipc(&shell, "railRenameFieldShown", &[], cx).stdout.trim(),
        "true"
    );
    cx.simulate_keystrokes("x");
    cx.run_until_parked();
    assert_eq!(
        ipc(&shell, "railRenameEditorText", &[], cx).stdout.trim(),
        "x"
    );
    // The NETWORK join now consumes the commit: it writes the captured share's label into a sandboxed bookmarks file.
    let sandbox = crate::window::network::tests::sandbox_bookmarks(&shell, cx);
    cx.simulate_keystrokes("enter");
    frame(cx);
    let written = crate::window::network::tests::written_bookmarks(sandbox, cx);
    assert_eq!(
        written.trim_end(),
        "smb://nas/share x",
        "the commit names the captured share and its new label"
    );
    assert!(
        !heard
            .borrow()
            .iter()
            .any(|request| matches!(request.event, Event::Rename { .. })),
        "a consumed rename is not re-emitted as an unowned request"
    );
    assert_eq!(
        ipc(&shell, "railRenamingIndex", &[], cx).stdout.trim(),
        "-1"
    );
    cx.simulate_keystrokes("f2");
    frame(cx);
    rows(&shell, vec![row("Home", "/home/test")], cx);
    assert_eq!(
        ipc(&shell, "railRenameEditorLive", &[], cx).stdout.trim(),
        "false"
    );
    assert!(
        !producer(&shell, cx).update(cx, |producer, cx| producer.start_rename(0, cx)),
        "F2 never renames a Favorite or Home row"
    );
}

#[gpui::test]
fn u08_failed_favorite_write_keeps_the_old_rows_and_reports_the_failure(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    shell.update(cx, |shell, cx| {
        shell.rail_state(
            &jsondoc::parse(r#"{"places":{"favourites":[{"label":"Docs","path":"/docs"}]}}"#)
                .unwrap(),
            cx,
        )
    });
    frame(cx);
    let producer = producer(&shell, cx);
    let service = producer.read_with(cx, |producer, _| producer.places.clone());
    service.update(cx, |places, _| places.exe = "/bin/false".into());
    producer.update(cx, |producer, cx| {
        producer.write_favourite(
            &flea::ui_logic_pl4::favourites::Operation::Rename {
                index: 0,
                label: "Lost".into(),
            },
            cx,
        )
    });
    cx.run_until_parked();
    frame(cx);
    service.read_with(cx, |service, _| {
        assert!(!service.favourites.busy);
        assert!(service.said.is_some());
    });
    let labels = ipc(&shell, "railLabels", &[], cx).stdout;
    assert!(labels.contains("Docs"));
    assert!(!labels.contains("Lost"));
}

#[gpui::test]
fn u08_repainting_keeps_text_cache_hits_and_trash_activation_reaches_its_owner(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(false, cx);
    rows(&shell, vec![flea::ui_logic_pl4::order::trash_entry()], cx);
    let producer = producer(&shell, cx);
    let before = producer.read_with(cx, |producer, _| producer.model.borrow().shape_misses());
    for _ in 0..5 {
        frame(cx);
    }
    assert_eq!(
        producer.read_with(cx, |producer, _| producer.model.borrow().shape_misses()),
        before
    );
    let heard = Rc::new(RefCell::new(Vec::<Request>::new()));
    let sink = heard.clone();
    cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, request: &Request, _| {
            sink.borrow_mut().push(request.clone())
        })
        .detach()
    });
    producer.update(cx, |producer, cx| producer.activate(0, cx));
    frame(cx);
    assert!(heard
        .borrow()
        .iter()
        .any(|request| request.event == Event::Trash && request.side == 0));
}

#[gpui::test]
fn u08_recent_history_uses_the_existing_reader_and_hands_real_visits_to_navigation(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(false, cx);
    let producer = producer(&shell, cx);
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/g2/pl4/recent-fixture.xbel"
    );
    producer.update(cx, |producer, cx| producer.read_recent(path.into(), cx));
    cx.run_until_parked();
    frame(cx);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.recent.on,
            "the service's result reaches the Recent owner"
        );
        assert_eq!(host.recent.paths, ["/newer.txt", "/older.txt"]);
    });
    producer.update(cx, |producer, cx| {
        producer.read_recent("/nonexistent/u08-history".into(), cx)
    });
    cx.run_until_parked();
    frame(cx);
    shell.read_with(cx, |shell, _| {
        assert!(shell.panes[0].as_ref().unwrap().recent.paths.is_empty());
    });
}

#[gpui::test]
fn u08_fix1_new_recent_request_cancels_pending_origin_and_keeps_its_own_id(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(false, cx);
    let producer = producer(&shell, cx);
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/g2/pl4/recent-fixture.xbel"
    );
    shell.update(cx, |shell, cx| {
        shell.view = crate::window::startup::View::Dual;
        shell.panes[1] = Some(crate::window::pane::PaneHost::new(
            "/right".into(),
            None,
            620.,
            &shell.locale,
        ));
        producer.update(cx, |producer, cx| producer.read_recent(fixture.into(), cx));
        shell.focus_pane(1, cx);
        producer.update(cx, |producer, cx| {
            producer.read_recent("/nonexistent/u08-fix1-history".into(), cx)
        });
        shell.focus_pane(0, cx);
    });
    cx.run_until_parked();
    frame(cx);
    shell.read_with(cx, |shell, _| {
        let right = shell.panes[1].as_ref().unwrap();
        assert!(right.recent.on, "the second ask completes on its own side");
        assert!(
            right.recent.paths.is_empty(),
            "the superseded request must never publish"
        );
        assert!(!shell.panes[0].as_ref().unwrap().recent.on);
    });
    producer.update(cx, |producer, cx| producer.read_recent(fixture.into(), cx));
    cx.run_until_parked();
    frame(cx);
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().recent.paths.len(), 2);
    });
}

#[gpui::test]
fn u08_fix1_deferred_requests_keep_the_enqueue_pane_and_distinct_ids(cx: &mut TestAppContext) {
    let (shell, cx) = open(false, cx);
    rows(&shell, vec![flea::ui_logic_pl4::order::trash_entry()], cx);
    let heard = Rc::new(RefCell::new(Vec::<Request>::new()));
    let sink = heard.clone();
    cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, request: &Request, _| {
            sink.borrow_mut().push(request.clone())
        })
        .detach()
    });
    shell.update(cx, |shell, cx| {
        shell.view = crate::window::startup::View::Dual;
        shell.panes[1] = Some(crate::window::pane::PaneHost::new(
            "/right".into(),
            None,
            620.,
            &shell.locale,
        ));
        shell
            .rail
            .as_ref()
            .unwrap()
            .places
            .update(cx, |producer, cx| {
                producer.activate(0, cx);
                producer.activate(0, cx);
            });
        shell.focus_pane(1, cx);
    });
    frame(cx);
    let requests = heard.borrow();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| request.side == 0));
    assert_eq!((requests[0].id, requests[1].id), (1, 2));
    assert_eq!(requests[0].event, Event::Trash);
    assert_eq!(requests[1].event, Event::Trash);
}

#[rustfmt::skip]
mod r2_probes {
    use super::*;

    #[gpui::test]
    fn review_u08_r2_refresh_preserves_manual_scroll_when_cursor_does_not_move(
        cx: &mut TestAppContext,
    ) {
        let (shell, cx) = open(false, cx);
        let mut entries: Vec<_> = (0..100)
            .map(|i| row(&format!("Place {i}"), &format!("/place/{i}")))
            .collect();
        rows(&shell, entries.clone(), cx);
        let producer = producer(&shell, cx);
        let position = producer.read_with(cx, |p, _| {
            let laid = p.laid.borrow();
            let rect = laid.row(2).unwrap().rect;
            point(px(rect.x + 20.), px(rect.y + rect.height / 2.))
        });
        cx.simulate_event(gpui::ScrollWheelEvent {
            position,
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-1000.))),
            modifiers: Modifiers::none(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        frame(cx);
        let before = producer.read_with(cx, |p, _| p.model.borrow().scroll);
        assert!(before > 900., "the real wheel event must scroll the rail");
        assert_eq!(producer.read_with(cx, |p, _| p.cursor()), Some(0));
        entries[99].label = "Updated place".into();
        rows(&shell, entries, cx);
        assert_eq!(producer.read_with(cx, |p, _| p.cursor()), Some(0));
        let after = producer.read_with(cx, |p, _| p.model.borrow().scroll);
        eprintln!("manual-scroll snapshot: before={before}, after={after}, cursor=0");
        assert_eq!(after, before, "a label refresh must not undo manual scrolling");
    }

    #[gpui::test]
    fn review_u08_r2_double_click_after_erase_before_paint_does_not_panic(
        cx: &mut TestAppContext,
    ) {
        let (shell, cx) = open(false, cx);
        rows(
            &shell,
            vec![RailEntry {
                group: Group::Network,
                kind: Kind::Share,
                label: "NAS share".into(),
                uri: "smb://nas/share".into(),
                glyph: "server",
                ..RailEntry::default()
            }],
            cx,
        );
        shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
        cx.simulate_keystrokes("f2");
        frame(cx);
        let producer = producer(&shell, cx);
        let position = producer.read_with(cx, |p, _| {
            let model = p.model.borrow();
            let laid = p.laid.borrow();
            let rect = laid.row(0).unwrap().editor.unwrap();
            point(
                px(rect.x + 4. + model.editor_x(5) - model.editor_scroll),
                px(rect.y + rect.height / 2.),
            )
        });
        cx.update(|window, cx| {
            window.dispatch_keystroke(gpui::Keystroke::parse("backspace").unwrap(), cx);
            assert_eq!(producer.read(cx).rename_text(), "");
            eprintln!("editor text erased through the real key route; double-click before next paint");
            window.dispatch_event(
                gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                    position,
                    button: MouseButton::Left,
                    modifiers: Modifiers::none(),
                    click_count: 2,
                    first_mouse: false,
                }),
                cx,
            );
        });
        frame(cx);
        assert_eq!(producer.read_with(cx, |p, _| p.rename_text()), "");
    }
}

#[path = "rail_origin_tests.rs"]
mod origin;
