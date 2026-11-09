//! Fork-shaped chords and the live key pair through the mounted tab model.
use super::*;

#[gpui::test]
fn fork_shaped_chords_and_the_live_pair_reach_the_window(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let shell = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    let observed = Rc::new(std::cell::RefCell::new(Vec::new()));
    let collected = observed.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, event: &ActionEvent, _| {
            collected
                .borrow_mut()
                .push((event.action, event.pair_armed));
        })
    });
    cx.run_until_parked();
    let event = |key: &str, text: Option<&str>, control: bool| KeyDownEvent {
        keystroke: Keystroke {
            key: key.into(),
            key_char: text.map(str::to_owned),
            modifiers: Modifiers {
                control,
                ..Default::default()
            },
            is_keypad: false,
            us_key: None,
        },
        is_held: false,
        prefer_character_input: false,
    };
    cx.simulate_modifiers_change(Modifiers {
        control: true,
        shift: true,
        ..Default::default()
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.key(&event(">", Some(">"), true), window, cx);
            assert_eq!(
                shell
                    .launch
                    .settings
                    .data
                    .get("hidden")
                    .and_then(flea::jsondoc::Json::as_bool),
                Some(true)
            );
            // Finish the hidden-file reread before testing the text-size and tab keys on a live listing.
            for reply in [
                Reply::Listed {
                    path: "/files".into(),
                    total: 1,
                },
                Reply::Rows {
                    start: 0,
                    rows: vec![flea::protocol::RowFacts::default()],
                    listing: 3,
                },
            ] {
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
            }
            let host = shell.panes[0].as_mut().unwrap();
            if host.pane.busy() {
                host.pane.tick(host.pane.now() + 1000);
                host.changed(&shell.locale);
            }
            assert!(!host.pane.busy(), "the chord runs on the answered listing");
            shell.key(&event("+", Some("+"), true), window, cx);
            assert_eq!(
                flea::ui_logic_settings_pages::pages::at(
                    &shell.launch.settings.data,
                    &["display", "textSize", "mode"]
                )
                .and_then(flea::jsondoc::Json::as_f64),
                Some(16.)
            );
            shell.key(&event("_", None, true), window, cx);
            assert_eq!(
                flea::ui_logic_settings_pages::pages::at(
                    &shell.launch.settings.data,
                    &["display", "textSize", "mode"]
                )
                .and_then(flea::jsondoc::Json::as_f64),
                Some(14.)
            );
        })
    });
    cx.simulate_modifiers_change(Modifiers::default());
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.launch.settings.data = flea::jsondoc::parse(r#"{"keys":"vim"}"#).unwrap();
            shell.with_tabs(|_, panes| {
                panes.visit(0, &mut |tabs, pane| {
                    let tab = tabs.snapshot(pane, None, 0, false);
                    tabs.tabs = Some(crate::tabs::state::pack(
                        vec![tab.clone(), tab.clone(), tab],
                        0,
                        None,
                    ));
                });
            });
            let host = shell.panes[0].as_ref().unwrap();
            let seat = flea::keys::sequence::Seat {
                listing: host.pane.held.listing as u64,
                cursor: host.pane.cursor,
                view: shell.view as u64,
                focus: shell.focus_side as u64,
                ..Default::default()
            };
            for (next, armed) in [(1, false), (2, true), (0, false)] {
                assert_eq!(
                    shell.keyboard.pair_armed("vim", seat),
                    armed,
                    "the live pair reaches the tab chord"
                );
                shell.key(&event("pagedown", None, true), window, cx);
                assert_eq!(
                    shell.strip.tabs[0].current(),
                    next,
                    "the mounted tab model runs the chord"
                );
                assert_eq!(shell.strip.tabs[0].count(), 3);
                if next == 1 {
                    shell.key(&event("y", Some("y"), false), window, cx);
                }
            }
        })
    });
    cx.run_until_parked();
    assert!(
        observed.borrow().is_empty(),
        "the mounted tab chords are consumed"
    );
}
