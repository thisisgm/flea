use super::*;

fn refuse(cx: &mut TestAppContext, handle: WindowHandle<Shell>, operation: &str, path: &str) {
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                crate::backend::Event::Refused {
                    operation: operation.into(),
                    path: path.into(),
                    message: "permission denied".into(),
                    mode: 0,
                },
                window,
                cx,
            );
        })
        .unwrap();
}

#[gpui::test]
fn u05_current_window_refusals_report_but_stale_paths_stay_quiet(cx: &mut TestAppContext) {
    let handle = open(cx);
    frame(cx, handle);
    refuse(cx, handle, "window", "/old");
    assert_eq!(read(cx, handle, "lastMessage"), Reply::value(""));
    refuse(cx, handle, "window", "/files");
    assert_eq!(read(cx, handle, "statusError"), Reply::value("true"));
    assert!(!read(cx, handle, "lastMessage").stdout.trim().is_empty());
    handle
        .update(cx, |shell, _, _| {
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.held.total, 1000);
        })
        .unwrap();
}

#[gpui::test]
fn u05_scan_failure_names_only_a_different_folders_leaf(cx: &mut TestAppContext) {
    for (path, expected) in [
        ("/files", "You don’t have permission to open this folder."),
        (
            "/else/child",
            "child cannot be opened: you do not have permission.",
        ),
    ] {
        let handle = open(cx);
        frame(cx, handle);
        handle
            .update(cx, |shell, _, cx| {
                let pane = &mut shell.panes[0].as_mut().unwrap().pane;
                assert!(pane.begin(path.into(), Default::default(), pane.now()));
                cx.notify();
            })
            .unwrap();
        refuse(cx, handle, "scan", path);
        assert_eq!(read(cx, handle, "lastMessage"), Reply::value(expected));
    }
}

#[gpui::test]
fn u05_refused_sort_is_a_notice_that_expires_without_dismissing_errors(cx: &mut TestAppContext) {
    let handle = open(cx);
    frame(cx, handle);
    refuse(cx, handle, "sort", "/files");
    assert_eq!(read(cx, handle, "statusError"), Reply::value("false"));
    let notice = read(cx, handle, "lastMessage");
    assert!(!notice.stdout.trim().is_empty());
    for _ in 0..260 {
        frame(cx, handle);
    }
    assert_eq!(read(cx, handle, "lastMessage"), Reply::value(""));

    refuse(cx, handle, "window", "/files");
    let error = read(cx, handle, "lastMessage");
    assert_eq!(error, Reply::value("Permission denied."));
    let errors = |cx: &mut TestAppContext| {
        json(cx, handle, "statusActivityState")
            .get("errors")
            .unwrap()
            .as_f64()
    };
    assert_eq!(errors(cx), Some(1.));
    refuse(cx, handle, "sort", "/files");
    assert_eq!(read(cx, handle, "lastMessage"), error);
    assert_eq!(read(cx, handle, "statusError"), Reply::value("true"));
    assert_eq!(errors(cx), Some(1.));
    for _ in 0..260 {
        frame(cx, handle);
    }
    assert_eq!(read(cx, handle, "lastMessage"), error);
    assert_eq!(read(cx, handle, "statusError"), Reply::value("true"));
    assert_eq!(errors(cx), Some(1.));

    escape(cx, handle);
    assert_eq!(errors(cx), Some(0.));
    assert_eq!(read(cx, handle, "statusError"), Reply::value("false"));
    assert_eq!(read(cx, handle, "lastMessage"), notice);
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(
        flea::ui_logic_fb1::messages::MESSAGE_MS - 1,
    ));
    cx.run_until_parked();
    assert_eq!(read(cx, handle, "lastMessage"), notice);
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(read(cx, handle, "lastMessage"), Reply::value(""));
}

#[gpui::test]
fn u05_status_escape_leaves_other_keyboard_contexts_their_own_cancel(cx: &mut TestAppContext) {
    use flea::keys::Context as KeyContext;
    const DRIVEN: [KeyContext; 4] = [
        KeyContext::Rail,
        KeyContext::Menu,
        KeyContext::Panel,
        KeyContext::Preview,
    ];
    // These contexts have no production Shell host to activate or cancel yet.
    const UNHOSTED: [KeyContext; 3] = [KeyContext::Pdf, KeyContext::Media, KeyContext::Editor];
    let accounted = [KeyContext::Listing]
        .into_iter()
        .chain(DRIVEN)
        .chain(UNHOSTED)
        .collect::<Vec<_>>();
    assert_eq!(KeyContext::ALL.as_slice(), accounted.as_slice());
    for context in KeyContext::ALL {
        if context == KeyContext::Listing {
            continue;
        }
        let (backend, mut requests) = crate::backend::Backend::test_lines();
        let handle = cx.add_window(|window, cx| {
            let shell = crate::state::tests::shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        });
        frame(cx, handle);
        refuse(cx, handle, "window", "/files");
        let error = read(cx, handle, "lastMessage");
        handle
            .update(cx, |shell, window, cx| {
                match context {
                    KeyContext::Rail => {
                        shell.mount_rail(cx);
                        shell.rail_focus(true, cx);
                        shell.route_rail(window, cx);
                        assert!(shell.rail.as_ref().unwrap().places.read(cx).model.borrow().focused);
                        assert_eq!(shell.focus_view(window, cx), "rail");
                    }
                    KeyContext::Menu => {
                        shell.open_menu(0, Some(0), (20., 60.), window, cx);
                        let id = shell.menu.captured.as_ref().unwrap().id;
                        for line in [
                            format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[32768]}}"#),
                            format!(r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[]}}"#),
                        ] {
                            shell.receive(0, crate::backend::Event::Producer(line), window, cx);
                        }
                        assert!(shell.menu_opened());
                        assert!(shell.panes[0].as_ref().unwrap().nav.gate.menu_open);
                    }
                    KeyContext::Panel => {
                        shell.view = crate::window::startup::View::Dual;
                        assert!(shell.chrome.view.is_some());
                        assert!(shell.dual_tab(false, window, cx));
                        assert_eq!(shell.focus_view(window, cx), "chrome");
                        assert!(shell.dual_focus.chrome.contains_focused(window, cx));
                        assert!(shell.chrome.view.as_ref().unwrap().read(cx).focus[9].is_focused(window));
                    }
                    KeyContext::Preview => {
                        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
                        assert!(pane.begin("/files".into(), Default::default(), pane.now()));
                        for reply in [
                            flea::protocol::Reply::Listed {
                                path: "/files".into(),
                                total: 1,
                            },
                            flea::protocol::Reply::Rows {
                                start: 0,
                                listing: 5,
                                rows: vec![flea::protocol::RowFacts {
                                    name: "notes.txt".into(),
                                    ..Default::default()
                                }],
                            },
                        ] {
                            shell.receive(0, crate::backend::Event::Reply(reply), window, cx);
                        }
                        shell.view = crate::window::startup::View::Columns;
                        shell.sync_selection_previews(window, cx);
                        let preview = shell.selection_preview(0).expect("the Columns preview host");
                        assert!(preview.read(cx).visible);
                        shell.key(
                            &gpui::KeyDownEvent {
                                keystroke: gpui::Keystroke::parse("ctrl-tab").unwrap(),
                                is_held: false,
                                prefer_character_input: false,
                            },
                            window,
                            cx,
                        );
                        assert!(preview.read(cx).focus.is_focused(window));
                    }
                    _ if UNHOSTED.contains(&context) => shell.keyboard.context = context,
                    _ => unreachable!(),
                }
                assert_eq!(shell.keyboard.context, context);
            })
            .unwrap();
        escape(cx, handle);
        cx.run_until_parked();
        handle
            .update(cx, |shell, window, cx| {
                match context {
                    KeyContext::Rail => {
                        assert!(
                            !shell
                                .rail
                                .as_ref()
                                .unwrap()
                                .places
                                .read(cx)
                                .model
                                .borrow()
                                .focused
                        );
                        assert_eq!(shell.focus_view(window, cx), "list");
                    }
                    KeyContext::Menu => {
                        assert!(!shell.menu_opened());
                        assert!(shell.menu.captured.is_none());
                        assert!(!shell.panes[0].as_ref().unwrap().nav.gate.menu_open);
                        assert!(std::iter::from_fn(&mut requests).any(|line| {
                            let doc = flea::jsondoc::parse(&line).unwrap();
                            doc.get("op").and_then(flea::jsondoc::Json::as_str) == Some("close")
                        }));
                    }
                    KeyContext::Panel => {
                        assert!(!shell.dual_focus.chrome.is_focused(window));
                        assert_eq!(shell.focus_view(window, cx), "list");
                    }
                    KeyContext::Preview => {
                        let preview = shell.selection_preview(0).unwrap();
                        assert!(preview.read(cx).visible);
                        assert!(!preview.read(cx).focus.is_focused(window));
                    }
                    _ if UNHOSTED.contains(&context) => {
                        assert_eq!(shell.keyboard.context, context);
                        shell.keyboard.context = KeyContext::Listing;
                    }
                    _ => unreachable!(),
                }
                assert!(shell.focus.is_focused(window));
                assert_eq!(shell.keyboard.context, KeyContext::Listing);
            })
            .unwrap();
        assert_eq!(read(cx, handle, "statusError"), Reply::value("true"));
        assert_eq!(read(cx, handle, "lastMessage"), error);
        escape(cx, handle);
        assert_eq!(read(cx, handle, "statusError"), Reply::value("false"));
    }
}

#[gpui::test]
fn u05_a_committed_folder_change_reports_the_lost_arm_once(cx: &mut TestAppContext) {
    use crate::g2::fb1::line::ArmLost;
    use std::{cell::RefCell, rc::Rc};
    let handle = open(cx);
    frame(cx, handle);
    let line = handle
        .update(cx, |shell, _, cx| {
            shell
                .footer(cx)
                .read_with(cx, |footer, _| footer.line.clone())
        })
        .unwrap();
    let lost = Rc::new(RefCell::new(Vec::new()));
    let seen = lost.clone();
    let _keep = cx.update(|cx| {
        cx.subscribe(&line, move |_, event: &ArmLost, _| {
            seen.borrow_mut().push(event.0)
        })
    });
    handle
        .update(cx, |shell, window, cx| {
            shell.status_messages(cx, |m, now| {
                m.set_arm_owner(37, now.max(1), now);
                m.say_arm("Press d again", 1500, now);
            });
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
                    listing: 3,
                },
            ] {
                shell.receive(0, crate::backend::Event::Reply(reply), window, cx);
            }
        })
        .unwrap();
    frame(cx, handle);
    frame(cx, handle);
    assert_eq!(lost.borrow().len(), 1);
    assert_eq!(lost.borrow()[0].owner, 37);
    assert_eq!(read(cx, handle, "statusPrimary"), Reply::value(""));
}

#[gpui::test]
fn u05_filter_and_search_escape_reaches_its_owner_without_acknowledging_an_error(
    cx: &mut TestAppContext,
) {
    use std::{cell::RefCell, rc::Rc};
    for owner in 0..3 {
        let handle = open(cx);
        frame(cx, handle);
        refuse(cx, handle, "window", "/files");
        let entity = handle.update(cx, |_, _, cx| cx.entity()).unwrap();
        let actions = Rc::new(RefCell::new(Vec::new()));
        let seen = actions.clone();
        let _keep = cx.update(|cx| {
            cx.subscribe(
                &entity,
                move |_, event: &crate::window::input::ActionEvent, _| {
                    seen.borrow_mut().push(event.action)
                },
            )
        });
        handle
            .update(cx, |shell, _, _| {
                let pane = &mut shell.panes[0].as_mut().unwrap().pane;
                pane.state.search = owner == 0;
                pane.swap.session.open = owner == 1;
                pane.state.query = if owner == 2 {
                    "notes".into()
                } else {
                    String::new()
                };
            })
            .unwrap();
        escape(cx, handle);
        cx.run_until_parked();
        assert_eq!(read(cx, handle, "statusError"), Reply::value("true"));
        assert_eq!(*actions.borrow(), ["escape"]);
    }
}

#[gpui::test]
fn u05_long_errors_reserve_the_entire_dismiss_hint_in_a_narrow_lane(cx: &mut TestAppContext) {
    use flea::ui_logic_fb1::status::Hint;
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), gpui::size(gpui::px(600.), gpui::px(400.)));
    frame(cx, handle);
    handle
        .update(cx, |shell, _, cx| {
            shell.status_messages(cx, |m, now| {
                m.say(&"failure ".repeat(200), true, "", Hint::None, now)
            });
        })
        .unwrap();
    frame(cx, handle);
    let footer = json(cx, handle, "statusFooterState");
    let hint = footer.get("secondary").unwrap();
    let lane = footer.get("lane").unwrap();
    let n = |value: &flea::jsondoc::Json, key| value.get(key).unwrap().as_f64().unwrap();
    assert_eq!(hint.get("text").unwrap().as_str(), Some(" · esc dismisses"));
    assert!(n(hint, "width") >= n(hint, "implicitWidth") - 1.);
    assert!(n(hint, "x") >= n(lane, "x"));
    assert!(n(hint, "x") + n(hint, "width") <= n(lane, "x") + n(lane, "width") + 1.);
}
