//! The menu keeps its own keyboard behavior and exposes the same hints it paints.
use super::{
    menus_tests::{actions, answer, entry, settle},
    test_shell::{land, open_root_with},
};
use crate::window::u07_tests::read;
use gpui::{KeyDownEvent, Keystroke, Modifiers, TestAppContext};

const FILE: u32 = 0o100644;

// S11J-5: Tab and Shift+Tab move the menu highlight as Down and Up do, and the menu keeps the keyboard.
#[gpui::test]
fn s11j5_tab_shift_tab_and_backtab_walk_the_menu_rows(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""keys":"default","view":"list""#, None);
    land(&shell, cx, "/files", vec![entry("notes.txt", FILE)], 1);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[FILE]);
    let copy = actions(&shell, cx)
        .iter()
        .position(|action| action == "copy")
        .unwrap();
    while read(&shell, cx, "contextMenuCursor", &[])
        .parse::<usize>()
        .unwrap()
        < copy
    {
        cx.simulate_keystrokes("down");
    }
    let cursor = |cx: &mut gpui::VisualTestContext| read(&shell, cx, "contextMenuCursor", &[]);
    cx.simulate_keystrokes("down");
    let below = cursor(cx);
    cx.simulate_keystrokes("up up");
    let above = cursor(cx);
    cx.simulate_keystrokes("down");
    assert_eq!(cursor(cx), copy.to_string());
    for (key, shift, want) in [
        ("tab", false, below),
        ("tab", true, copy.to_string()),
        ("backtab", true, above),
    ] {
        cx.simulate_event(KeyDownEvent {
            keystroke: Keystroke {
                key: key.into(),
                modifiers: Modifiers {
                    shift,
                    ..Modifiers::none()
                },
                ..Keystroke::default()
            },
            is_held: false,
            prefer_character_input: false,
        });
        settle(cx);
        assert_eq!(cursor(cx), want, "{key}, shift={shift}");
        assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    }
    // A modified Tab moves nothing and leaves the menu holding the keyboard.
    let (menu, stop) = (
        read(&shell, cx, "menuState", &[]),
        read(&shell, cx, "focusStop", &[]),
    );
    for modifiers in [
        Modifiers {
            control: true,
            ..Modifiers::none()
        },
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
        Modifiers {
            control: true,
            shift: true,
            ..Modifiers::none()
        },
    ] {
        cx.simulate_event(KeyDownEvent {
            keystroke: Keystroke {
                key: "tab".into(),
                modifiers,
                ..Keystroke::default()
            },
            is_held: false,
            prefer_character_input: false,
        });
        settle(cx);
        assert_eq!(read(&shell, cx, "menuState", &[]), menu, "{modifiers:?}");
        assert_eq!(read(&shell, cx, "focusStop", &[]), stop, "{modifiers:?}");
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// The file-menu-print board pins Open with no key while key hints are on: Enter activates whichever row holds the cursor.
#[gpui::test]
fn ey177_open_draws_no_enter_hint_when_key_hints_are_on(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(
        cx,
        r#""keys":"default","view":"list","keyHints":true"#,
        None,
    );
    land(&shell, cx, "/files", vec![entry("notes.txt", FILE)], 1);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[FILE]);
    let shown = actions(&shell, cx);
    let hints = read(&shell, cx, "contextMenuHints", &[]);
    let hints = hints.split('|').collect::<Vec<_>>();
    for (action, hint) in [("open", ""), ("cut", "x"), ("copy", "y"), ("rename", "r")] {
        let index = shown.iter().position(|row| row == action).unwrap();
        assert_eq!(hints[index], hint, "the painted hint for {action}");
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
