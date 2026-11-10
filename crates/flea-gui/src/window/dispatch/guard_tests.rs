//! Every listing action the key table binds is run by a handler, or is named here by the family that will run it.
use super::test_shell::{keystroke, land, open, row};
use crate::window::input::{event_input, ActionEvent};
use flea::keys::{self, Context, Frontend, Match};
use gpui::{KeyDownEvent, Keystroke, Modifiers, TestAppContext};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

// Actions a key still hands to nobody, by the family that will run them, cited in v0.3.12 ui/js/Focus.js unless another file is named; a family removes its line when it lands.
#[rustfmt::skip]
const NOT_YET: &[(&str, &[&str])] = &[
];

// Spawns a real window process, so the guard reads it from the table instead of pressing it.
const SPAWNS: &[&str] = &["windowNew"];

// Never handed to nobody, on any row.
const NAVIGATION: &[&str] = &["parent", "historyBack", "historyForward", "reload"];

// Never handed to nobody on a folder row; on a file row the opener runs open and turns pageForward into preview.
const FOLDER_ROW: &[&str] = &["open", "pageForward"];

#[gpui::test]
fn print_key_opens_printable_rows_and_keeps_its_refusal(cx: &mut TestAppContext) {
    for preset in keys::PRESETS {
        for (name, directory, printable) in [
            ("folder", true, false),
            ("other.bin", false, false),
            ("report.pdf", false, true),
        ] {
            let (shell, cx) = open(cx, preset);
            land(&shell, cx, "/files", vec![row(name, directory)], 1);
            super::test_shell::sent();
            let emitted = Rc::new(RefCell::new(Vec::new()));
            let sink = emitted.clone();
            let _subscription = cx.update(|_, cx| {
                cx.subscribe(&shell, move |_, event: &ActionEvent, _| {
                    sink.borrow_mut().push(event.action)
                })
            });
            cx.simulate_keystrokes("ctrl-p");
            cx.run_until_parked();
            assert!(
                emitted.borrow().is_empty(),
                "{preset} {name}: Print owns both an opened card and a refused selection"
            );
            shell.update(cx, |shell, cx| {
                assert_eq!(shell.output_showing(cx), printable, "{preset} {name}");
                if !printable {
                    let expected = shell.locale.catalog.tr("dialogs.output.error.print_kind");
                    let shown =
                        shell.status_messages(cx, |messages, _| messages.shown().text.to_string());
                    assert_eq!(shown, expected, "Print keeps its specific refusal");
                }
            });
            let queries = super::test_shell::sent()
                .iter()
                .filter(|line| line.contains(r#""c":"printoptions""#))
                .count();
            assert_eq!(queries, usize::from(printable), "{preset} {name}");
            shell.update(cx, |shell, _| shell.stop_for_app_quit());
            cx.update(|window, _| window.remove_window());
            cx.run_until_parked();
        }
    }
}

// One fresh production Shell per press, so no earlier key's state decides a later one.
fn handed_to_nobody(
    cx: &mut TestAppContext,
    preset: &str,
    event: &KeyDownEvent,
    twice: bool,
    cursor: usize,
) -> Vec<&'static str> {
    let (shell, cx) = open(cx, preset);
    let rows = vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ];
    land(&shell, cx, "/files", rows, 1);
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.panes[0].as_mut().unwrap().pane.cursor = cursor
        })
    });
    let emitted = Rc::new(RefCell::new(Vec::new()));
    let sink = emitted.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, event: &ActionEvent, _| {
            sink.borrow_mut().push(event.action)
        })
    });
    for _ in 0..1 + usize::from(twice) {
        cx.simulate_event(event.clone());
        cx.run_until_parked();
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    let emitted = emitted.borrow().clone();
    emitted
}

#[gpui::test]
fn every_bound_listing_action_has_a_handler_or_a_named_family(cx: &mut TestAppContext) {
    let mut pressed = BTreeSet::new();
    let mut unreachable = Vec::new();
    let mut folder = BTreeSet::new();
    let mut file = BTreeSet::new();
    for preset in keys::PRESETS {
        for binding in keys::binding_rows(preset, Frontend::Gui) {
            if SPAWNS.contains(&binding.action) {
                continue;
            }
            let event = keystroke(binding);
            let input = event_input(&event, Modifiers::none());
            if keys::lookup(preset, Context::Listing, Frontend::Gui, input)
                != Match::Action(binding.action)
            {
                unreachable.push(format!("{preset} {} {}", binding.keys, binding.action));
                continue;
            }
            let stamp = (
                event.keystroke.key.clone(),
                event.keystroke.key_char.clone(),
                binding.mask,
                event.keystroke.is_keypad,
                binding.action,
            );
            if !pressed.insert(stamp) {
                continue;
            }
            let twice = keys::SEQUENCES
                .iter()
                .any(|sequence| sequence.paired && sequence.arm == binding.action);
            folder.extend(handed_to_nobody(cx, preset, &event, twice, 0));
            file.extend(handed_to_nobody(cx, preset, &event, twice, 2));
        }
    }
    for digit in keys::DIGITS {
        let event = KeyDownEvent {
            keystroke: Keystroke {
                key: digit.text.into(),
                key_char: Some(digit.text.into()),
                modifiers: Modifiers::none(),
                is_keypad: false,
                us_key: None,
            },
            is_held: false,
            prefer_character_input: false,
        };
        folder.extend(handed_to_nobody(cx, "default", &event, false, 0));
    }
    assert_eq!(
        unreachable,
        Vec::<String>::new(),
        "a row no keystroke reaches"
    );
    for action in NAVIGATION {
        assert!(
            !folder.contains(action) && !file.contains(action),
            "{action} is the navigation family's"
        );
    }
    for action in FOLDER_ROW {
        assert!(!folder.contains(action), "{action} opens a folder row");
        assert!(
            !file.contains(action),
            "{action} on a file row is the opener's"
        );
    }
    let named: BTreeSet<&str> = NOT_YET
        .iter()
        .flat_map(|(_, actions)| actions.iter().copied())
        .collect();
    let emitted: BTreeSet<&str> = folder.union(&file).copied().collect();
    assert_eq!(
        emitted.difference(&named).collect::<Vec<_>>(),
        Vec::<&&str>::new(),
        "a bound action runs nowhere: give it a handler or name its family"
    );
    assert_eq!(
        named.difference(&emitted).collect::<Vec<_>>(),
        Vec::<&&str>::new(),
        "a named action now runs: remove it from NOT_YET"
    );
}
