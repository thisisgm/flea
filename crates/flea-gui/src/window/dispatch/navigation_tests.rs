//! Return, the keypad Enter, a double click and BackSpace navigate the production Shell with no test subscriber.
use super::test_shell::{asked, keypad_enter, land, open, row, set_cursor};
use crate::window::u07_tests::{press, read};
use gpui::TestAppContext;

#[gpui::test]
fn return_keypad_enter_double_click_and_backspace_navigate_the_production_shell(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    shell.read_with(cx, |shell, _| {
        assert!(
            shell.rail.is_some() && shell.trash.is_some(),
            "the native launch's producers are live"
        );
    });
    let home = || {
        vec![
            row("alpha", true),
            row("beta", true),
            row("notes.txt", false),
        ]
    };
    land(&shell, cx, "/files", home(), 1);
    // The native symptom: Down moves the cursor, then Return leaves the path where it was.
    cx.simulate_keystrokes("down");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
        1
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(
        asked(&shell, cx),
        "/files/beta",
        "Return opens the folder under the cursor"
    );
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 2);
    cx.simulate_keystrokes("backspace");
    assert_eq!(
        asked(&shell, cx),
        "/files",
        "BackSpace climbs to the parent"
    );
    land(&shell, cx, "/files", home(), 3);
    set_cursor(&shell, cx, 1);
    keypad_enter(cx);
    assert_eq!(
        asked(&shell, cx),
        "/files/beta",
        "the keypad Enter opens the folder"
    );
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 4);
    cx.simulate_keystrokes("backspace");
    land(&shell, cx, "/files", home(), 5);
    let centre = read(&shell, cx, "rowCentre", &["0"]);
    press(cx, &centre, 1);
    assert_eq!(asked(&shell, cx), "/files", "one click only selects");
    press(cx, &centre, 2);
    assert_eq!(
        asked(&shell, cx),
        "/files/alpha",
        "a double click opens the folder"
    );
    land(&shell, cx, "/files/alpha", Vec::new(), 6);
    // A file row is the opener's: Enter there navigates nowhere.
    cx.simulate_keystrokes("backspace");
    land(&shell, cx, "/files", home(), 7);
    set_cursor(&shell, cx, 2);
    let lists = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.counters.lists
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(asked(&shell, cx), "/files");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .counters
            .lists),
        lists
    );
    // Focus.js: a reload answers from the rail as well as the list.
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    cx.simulate_keystrokes("tab");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.keyboard.context),
        flea::keys::Context::Rail
    );
    cx.simulate_keystrokes("f5");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .counters
            .lists),
        lists + 1,
        "F5 from the focused rail re-lists the pane"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
