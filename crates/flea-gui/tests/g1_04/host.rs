use super::*;
use crate::window::input::ActionEvent;
use gpui::{KeyDownEvent, Keystroke, Modifiers};

#[gpui::test]
fn generated_actions_reach_the_window_and_respect_context_and_preset(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let shell = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    let observed = Rc::new(std::cell::RefCell::new(Vec::new()));
    let collected = observed.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, event: &ActionEvent, _| {
            collected.borrow_mut().push(event.action);
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
        },
        is_held: false,
        prefer_character_input: false,
    };
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.key(&event("j", Some("j"), false), window, cx);
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 1);
            shell.keyboard.context = flea::keys::Context::Editor;
            shell.key(&event("down", None, false), window, cx);
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 1);
            shell.keyboard.context = flea::keys::Context::Listing;
            shell.launch.settings.data = flea::jsondoc::parse(r#"{"keys":"vim"}"#).unwrap();
            shell.key(&event("y", Some("y"), false), window, cx);
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.held = Rc::new(crate::list::pane::Held {
                listing: host.pane.held.listing + 1,
                total: host.pane.held.total,
                start: host.pane.held.start,
                rows: host.pane.held.rows.clone(),
            });
            shell.key(&event("y", Some("y"), false), window, cx);
            shell.keyboard.disarm();
            shell.launch.settings.data = flea::jsondoc::parse(r#"{"keys":"mac"}"#).unwrap();
            shell.key(&event("n", None, true), window, cx);
            shell.key(&event("t", None, true), window, cx);
            shell.key(&event("x", None, true), window, cx);
            shell.key(&event("3", None, true), window, cx);
            assert_eq!(shell.view, View::Grid);
        })
    });
    cx.run_until_parked();
    assert_eq!(*observed.borrow(), vec!["tabNew"]);
}

#[gpui::test]
fn fork_shaped_chords_and_the_live_pair_reach_the_window(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
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
            shell.key(&event("+", Some("+"), true), window, cx);
            shell.key(&event("_", None, true), window, cx);
        })
    });
    cx.simulate_modifiers_change(Modifiers::default());
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.launch.settings.data = flea::jsondoc::parse(r#"{"keys":"vim"}"#).unwrap();
            shell.key(&event("escape", None, false), window, cx);
            shell.key(&event("y", Some("y"), false), window, cx);
            shell.key(&event("escape", None, false), window, cx);
            shell.key(&event("escape", None, false), window, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(
        *observed.borrow(),
        vec![
            ("toggleHidden", false),
            ("textSizeUp", false),
            ("textSizeDown", false),
            ("escape", false),
            ("escape", true),
            ("escape", false),
        ]
    );
}
