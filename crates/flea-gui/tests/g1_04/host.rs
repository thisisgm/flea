use super::*;
use crate::window::input::ActionEvent;
use gpui::{Entity, KeyDownEvent, Keystroke, Modifiers, Subscription, VisualTestContext};

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

type Emitted = Rc<std::cell::RefCell<Vec<&'static str>>>;

fn shifted() -> Modifiers {
    Modifiers {
        shift: true,
        ..Default::default()
    }
}

fn controlled() -> Modifiers {
    Modifiers {
        control: true,
        ..Default::default()
    }
}

/// A shell on its listing, or mid-swap when `busy`, with every action its key handler emits recorded.
fn key_shell(
    cx: &mut TestAppContext,
    busy: bool,
) -> (&mut VisualTestContext, Entity<Shell>, Emitted, Subscription) {
    let (backend, _) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let shell = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    let observed = Rc::new(std::cell::RefCell::new(Vec::new()));
    let collected = observed.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, event: &ActionEvent, _| {
            collected.borrow_mut().push(event.action);
        })
    });
    cx.run_until_parked();
    if busy {
        // begin holds the pane mid-swap, the first thing navigate does before it asks the backend.
        cx.update(|_, cx| {
            shell.update(cx, |shell, _| {
                let host = shell.panes[0].as_mut().unwrap();
                let now = host.pane.now();
                let options = crate::pane::swap::Options::default();
                assert!(host.pane.begin("/next".into(), options, now));
            })
        });
    }
    cx.update(|_, cx| assert_eq!(shell.read(cx).panes[0].as_ref().unwrap().pane.busy(), busy));
    (cx, shell, observed, subscription)
}

fn press(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    key: &str,
    text: Option<&str>,
    modifiers: Modifiers,
) {
    let event = KeyDownEvent {
        keystroke: Keystroke {
            key: key.into(),
            key_char: text.map(str::to_owned),
            modifiers,
            is_keypad: false,
        },
        is_held: false,
        prefer_character_input: false,
    };
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.key(&event, window, cx)));
    cx.run_until_parked();
}

#[gpui::test]
fn a_busy_pane_swallows_a_modified_row_action(cx: &mut TestAppContext) {
    // Shift-Delete is deletePermanently and Ctrl-C is copy in the default preset: both are row actions.
    let (cx, shell, observed, _subscription) = key_shell(cx, true);
    press(cx, &shell, "delete", None, shifted());
    assert!(
        observed.borrow().is_empty(),
        "shift-delete: {:?}",
        observed.borrow()
    );
    press(cx, &shell, "c", None, controlled());
    assert!(
        observed.borrow().is_empty(),
        "ctrl-c: {:?}",
        observed.borrow()
    );
}

#[gpui::test]
fn a_busy_pane_swallows_a_plain_row_action(cx: &mut TestAppContext) {
    // F2 is rename and Delete is trash: neither may act on the listing being replaced.
    let (cx, shell, observed, _subscription) = key_shell(cx, true);
    press(cx, &shell, "f2", None, Modifiers::default());
    assert!(observed.borrow().is_empty(), "f2: {:?}", observed.borrow());
    press(cx, &shell, "delete", None, Modifiers::default());
    assert!(
        observed.borrow().is_empty(),
        "delete: {:?}",
        observed.borrow()
    );
}

#[gpui::test]
fn a_busy_pane_still_acts_on_the_answered_keys(cx: &mut TestAppContext) {
    // Escape and F5 emit their actions while a listing is out; Ctrl-3 switches the view inline.
    let (cx, shell, observed, _subscription) = key_shell(cx, true);
    press(cx, &shell, "escape", None, Modifiers::default());
    assert_eq!(*observed.borrow(), vec!["escape"]);
    press(cx, &shell, "f5", None, Modifiers::default());
    assert_eq!(*observed.borrow(), vec!["escape", "reload"]);
    press(cx, &shell, "3", None, controlled());
    cx.update(|_, cx| assert_eq!(shell.read(cx).view, View::Grid));
}

#[gpui::test]
fn an_idle_pane_emits_the_row_actions_the_busy_tests_hold(cx: &mut TestAppContext) {
    let (cx, shell, observed, _subscription) = key_shell(cx, false);
    press(cx, &shell, "delete", None, shifted());
    press(cx, &shell, "f2", None, Modifiers::default());
    press(cx, &shell, "c", None, controlled());
    assert_eq!(
        *observed.borrow(),
        vec!["deletePermanently", "rename", "copy"]
    );
}
