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
            us_key: None,
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
            assert_eq!(shell.strip.tabs[0].count(), 1);
            shell.key(&event("t", None, true), window, cx);
            assert_eq!(shell.strip.tabs[0].count(), 2);
            assert_eq!(shell.strip.tabs[0].current(), 1);
            shell.key(&event("x", None, true), window, cx);
            shell.key(&event("3", None, true), window, cx);
            assert_eq!(shell.view, View::Grid);
        })
    });
    cx.run_until_parked();
    assert!(
        observed.borrow().is_empty(),
        "the mounted tab action is consumed"
    );
}

#[path = "host_tab_pair.rs"]
mod tab_pair;

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
    assert_busy(cx, &shell, busy);
    (cx, shell, observed, subscription)
}

// The pane's backend records its lines, so a row action the file operations family runs is read back.
fn requests(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> impl FnMut() -> Vec<String> {
    let (backend, mut read) = Backend::test_lines();
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.panes[0].as_mut().unwrap().backend = Some(backend)
        })
    });
    move || {
        std::iter::from_fn(&mut read)
            .filter(|line| line.contains(r#""c":"trash""#) || line.contains(r#""c":"menuaction""#))
            .collect()
    }
}

fn assert_busy(cx: &mut VisualTestContext, shell: &Entity<Shell>, busy: bool) {
    cx.update(|_, cx| assert_eq!(shell.read(cx).panes[0].as_ref().unwrap().pane.busy(), busy));
}

fn cursor(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> usize {
    cx.update(|_, cx| shell.read(cx).panes[0].as_ref().unwrap().pane.cursor)
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
            us_key: None,
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
    let mut sent = requests(cx, &shell);
    press(cx, &shell, "delete", None, shifted());
    assert!(
        observed.borrow().is_empty(),
        "shift-delete: {:?}",
        observed.borrow()
    );
    assert_eq!(
        sent(),
        Vec::<String>::new(),
        "shift-delete reached the backend"
    );
    press(cx, &shell, "c", None, controlled());
    assert!(
        observed.borrow().is_empty(),
        "ctrl-c: {:?}",
        observed.borrow()
    );
    shell.read_with(cx, |shell, _| {
        assert!(
            !shell.clipboard.board.paths_pending(0),
            "ctrl-c reached the clipboard on a busy pane"
        )
    });
}

#[gpui::test]
fn a_busy_pane_swallows_a_plain_row_action(cx: &mut TestAppContext) {
    // F2 is rename and Delete is trash: neither may act on the listing being replaced.
    let (cx, shell, observed, _subscription) = key_shell(cx, true);
    let mut sent = requests(cx, &shell);
    press(cx, &shell, "f2", None, Modifiers::default());
    assert!(observed.borrow().is_empty(), "f2: {:?}", observed.borrow());
    press(cx, &shell, "delete", None, Modifiers::default());
    assert!(
        observed.borrow().is_empty(),
        "delete: {:?}",
        observed.borrow()
    );
    assert_eq!(sent(), Vec::<String>::new(), "delete reached the backend");
}

#[gpui::test]
fn a_busy_pane_still_acts_on_the_answered_keys(cx: &mut TestAppContext) {
    // Escape runs while a listing is out, F5 answers that one is loading, and Ctrl-3 switches the view inline.
    let (cx, shell, observed, _subscription) = key_shell(cx, true);
    assert_busy(cx, &shell, true);
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.status_messages(cx, |messages, now| {
                messages.say(
                    "Copied 1 item",
                    false,
                    "",
                    flea::ui_logic_fb1::status::Hint::None,
                    now,
                )
            })
        })
    });
    press(cx, &shell, "escape", None, Modifiers::default());
    let notice = cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.status_messages(cx, |messages, _| messages.notice().to_owned())
        })
    });
    assert_eq!(notice, "", "Escape clears the notice on a busy pane");
    assert!(
        observed.borrow().is_empty(),
        "the listing family runs Escape"
    );
    assert_busy(cx, &shell, true);
    press(cx, &shell, "f5", None, Modifiers::default());
    assert!(observed.borrow().is_empty());
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        assert_eq!(
            shell.panes[0].as_ref().unwrap().status.as_ref(),
            shell.locale.catalog.tr("pane.busy")
        );
    });
    assert_busy(cx, &shell, true);
    cx.update(|_, cx| assert_ne!(shell.read(cx).view, View::Grid));
    press(cx, &shell, "3", None, controlled());
    cx.update(|_, cx| assert_eq!(shell.read(cx).view, View::Grid));
}

#[gpui::test]
fn a_busy_pane_holds_its_cursor_against_down_and_shift_down(cx: &mut TestAppContext) {
    // Down moves the cursor on an idle pane; Shift-Down is extendDown. Neither may move or emit while busy.
    let (cx, shell, observed, _subscription) = key_shell(cx, true);
    let start = cursor(cx, &shell);
    assert_busy(cx, &shell, true);
    press(cx, &shell, "down", None, Modifiers::default());
    assert_eq!(
        cursor(cx, &shell),
        start,
        "down moved the cursor on a busy pane"
    );
    assert!(
        observed.borrow().is_empty(),
        "down: {:?}",
        observed.borrow()
    );
    assert_busy(cx, &shell, true);
    press(cx, &shell, "down", None, shifted());
    assert_eq!(
        cursor(cx, &shell),
        start,
        "shift-down moved the cursor on a busy pane"
    );
    assert!(
        observed.borrow().is_empty(),
        "shift-down: {:?}",
        observed.borrow()
    );
}

#[gpui::test]
fn an_idle_pane_moves_and_extends_selection_on_shift_down(cx: &mut TestAppContext) {
    // The control for the busy cursor test: the same two keys act on an idle pane.
    let (cx, shell, observed, _subscription) = key_shell(cx, false);
    let start = cursor(cx, &shell);
    press(cx, &shell, "down", None, Modifiers::default());
    assert_eq!(cursor(cx, &shell), start + 1);
    press(cx, &shell, "down", None, shifted());
    assert_eq!(cursor(cx, &shell), start + 2);
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().selected,
            [start + 1, start + 2]
        );
    });
    assert!(observed.borrow().is_empty());
}

#[gpui::test]
fn an_idle_pane_emits_the_row_actions_the_busy_tests_hold(cx: &mut TestAppContext) {
    // Shift-Delete runs in the file operations family, so it reaches the backend instead of the emitted list.
    let (cx, shell, observed, _subscription) = key_shell(cx, false);
    let mut sent = requests(cx, &shell);
    press(cx, &shell, "delete", None, shifted());
    let asked = sent();
    assert!(
        asked.len() == 1 && asked[0].contains(r#""op":"snapshot""#),
        "{asked:?}"
    );
    // Copy has an in-process owner now: it asks the backend for its rows instead of leaving an emit nobody hears.
    press(cx, &shell, "c", None, controlled());
    // Rename has an in-process owner: F2 opens the cursor row's editor instead of leaving an emit nobody hears.
    press(cx, &shell, "f2", None, Modifiers::default());
    assert!(observed.borrow().is_empty(), "{:?}", observed.borrow());
    shell.read_with(cx, |shell, _| {
        assert!(shell.clipboard.board.paths_pending(0));
        assert!(shell.rename.edit.is_some(), "F2 opened the editor")
    });
}
