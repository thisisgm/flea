//! S1A-3: the strip keys answer from the rail as they do from the list.
use super::*;

// S1A-3: with the keyboard in the rail a digit selects its tab and names a missing one, as it does from the list.
#[gpui::test]
fn a_digit_in_the_rail_selects_its_tab_and_names_a_missing_one(cx: &mut TestAppContext) {
    let (shell, cx) = two_tabs(cx, "default");
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    cx.simulate_keystrokes("tab");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.keyboard.context),
        Context::Rail
    );
    digit(cx, "1");
    assert_eq!(strip(&shell, cx).1, "0");
    assert_eq!(asked(&shell, cx), "/files");
    land(&shell, cx, "/files", home(), 3);
    digit(cx, "2");
    assert_eq!(strip(&shell, cx).1, "1");
    land(&shell, cx, "/files/alpha", vec![row("inner", true)], 4);
    digit(cx, "5");
    assert_eq!(message(&shell, cx), "No tab 5");
    assert_eq!(strip(&shell, cx).1, "1");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.keyboard.context),
        Context::Rail,
        "a tab key leaves the keyboard in the rail"
    );
    close(&shell, cx);
}
