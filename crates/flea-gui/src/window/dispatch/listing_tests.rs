//! s, S, o, Escape and a --select launch drive the production Shell with no test subscriber.
use super::test_shell::{asked, land, lists, open, open_with, row, set_cursor};
use crate::{state::Shell, window::u07_tests::read};
use flea::protocol::RowFacts;
use gpui::{Entity, KeyDownEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext};

fn home() -> Vec<RowFacts> {
    vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ]
}

fn mark(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.sort_mark()
    })
}

fn sorts(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> usize {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.counters.sorts
    })
}

fn cursor(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> usize {
    shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor)
}

fn selected(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<usize> {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().selected.clone()
    })
}

// A capital letter as Linux delivers it: the key, its character and Shift.
fn capital(cx: &mut VisualTestContext, letter: &str) {
    cx.simulate_event(KeyDownEvent {
        keystroke: Keystroke {
            key: letter.to_lowercase(),
            key_char: Some(letter.into()),
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::none()
            },
            is_keypad: false,
            us_key: None,
        },
        is_held: false,
        prefer_character_input: false,
    });
    cx.run_until_parked();
}

fn footer_left(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    let state = flea::jsondoc::parse(&read(shell, cx, "statusFooterState", &[])).unwrap();
    let left = state.get("left").expect("the footer's left lane");
    left.get("text")
        .and_then(|text| text.as_str())
        .unwrap()
        .to_owned()
}

fn sort_keys_resort(settings: &str, cx: &mut TestAppContext) {
    let (shell, cx) = open_with(cx, settings, None);
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    assert_eq!(mark(&shell, cx), "name:asc");
    let before = sorts(&shell, cx);
    cx.simulate_keystrokes("s");
    assert_eq!(mark(&shell, cx), "size:asc", "s steps to the next order");
    assert_eq!(sorts(&shell, cx), before + 1, "the sort request goes out");
    assert_eq!(
        cursor(&shell, cx),
        2,
        "the retained rows keep their cursor until replacement"
    );
    land(&shell, cx, "/files", home(), 2);
    assert_eq!(
        cursor(&shell, cx),
        0,
        "the sorted rows land on the first row"
    );
    capital(cx, "S");
    assert_eq!(
        mark(&shell, cx),
        "size:desc",
        "S reverses the order the listing is in"
    );
    assert_eq!(sorts(&shell, cx), before + 2);
    capital(cx, "S");
    assert_eq!(mark(&shell, cx), "size:asc", "S again reverses it back");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn s_and_shift_s_resort_the_list(cx: &mut TestAppContext) {
    sort_keys_resort(r#""keys":"default","view":"list""#, cx);
}

#[gpui::test]
fn s_and_shift_s_resort_the_grid(cx: &mut TestAppContext) {
    sort_keys_resort(r#""keys":"default","view":"grid""#, cx);
}

#[gpui::test]
fn reveal_is_quiet_outside_a_search(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 1);
    let before = lists(&shell, cx);
    cx.simulate_keystrokes("o");
    assert_eq!(asked(&shell, cx), "/files");
    assert_eq!(
        lists(&shell, cx),
        before,
        "o asks for nothing outside a search"
    );
    assert_eq!(cursor(&shell, cx), 1);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn escape_climbs_with_escape_up_on_and_keeps_marks_first(cx: &mut TestAppContext) {
    let (shell, cx) = open_with(
        cx,
        r#""keys":"default","view":"list","escapeUp":true"#,
        None,
    );
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("enter");
    land(
        &shell,
        cx,
        "/files/beta",
        vec![row("inner", true), row("two", true)],
        2,
    );
    // A deliberate mark owns the first Escape, as escapePressed clears it before any climb.
    cx.simulate_keystrokes("v");
    assert_eq!(selected(&shell, cx), vec![0]);
    cx.simulate_keystrokes("escape");
    assert_eq!(
        selected(&shell, cx),
        Vec::<usize>::new(),
        "Escape clears the mark"
    );
    assert_eq!(asked(&shell, cx), "/files/beta", "and stays in the folder");
    cx.simulate_keystrokes("escape");
    assert_eq!(asked(&shell, cx), "/files", "with no mark Escape climbs");
    land(&shell, cx, "/files", home(), 3);
    assert_eq!(
        selected(&shell, cx),
        vec![1],
        "the folder left is selected on landing"
    );
    // The landed row is no mark, so Escape keeps climbing through it.
    cx.simulate_keystrokes("escape");
    assert_eq!(asked(&shell, cx), "/", "Escape climbs through a landed row");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_deliberate_mark_on_the_landed_row_keeps_escape_home(cx: &mut TestAppContext) {
    let (shell, cx) = open_with(
        cx,
        r#""keys":"default","view":"list","escapeUp":true"#,
        None,
    );
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("enter");
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 2);
    cx.simulate_keystrokes("escape");
    land(&shell, cx, "/files", home(), 3);
    assert_eq!(
        selected(&shell, cx),
        vec![1],
        "the folder left is selected on landing"
    );
    // Selection.js promote: v on the landed row keeps it as a deliberate mark, no longer the landing.
    cx.simulate_keystrokes("v");
    assert_eq!(selected(&shell, cx), vec![1]);
    cx.simulate_keystrokes("escape");
    assert_eq!(
        selected(&shell, cx),
        Vec::<usize>::new(),
        "Escape clears the mark"
    );
    assert_eq!(asked(&shell, cx), "/files", "and stays in the folder");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn escape_stays_put_with_escape_up_off(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("enter");
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 2);
    let before = lists(&shell, cx);
    shell.update(cx, |shell, cx| {
        shell.status_messages(cx, |messages, now| {
            messages.say(
                "Copied 1 item",
                false,
                "",
                flea::ui_logic_status_feedback::status::Hint::None,
                now,
            )
        })
    });
    cx.simulate_keystrokes("escape");
    assert_eq!(asked(&shell, cx), "/files/beta");
    assert_eq!(
        lists(&shell, cx),
        before,
        "Escape asks for nothing with the setting off"
    );
    let notice = shell.update(cx, |shell, cx| {
        shell.status_messages(cx, |messages, _| messages.notice().to_owned())
    });
    assert_eq!(notice, "", "Escape clears the notice");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_select_launch_selects_and_reveals_the_asked_row(cx: &mut TestAppContext) {
    let (shell, cx) = open_with(cx, r#""keys":"default","view":"list""#, Some("/files/n39"));
    let rows = (0..76).map(|at| row(&format!("n{at:02}"), false)).collect();
    land(&shell, cx, "/files", rows, 1);
    assert_eq!(cursor(&shell, cx), 39, "the cursor lands on the asked row");
    assert_eq!(selected(&shell, cx), vec![39], "the asked row is selected");
    let (top, height, scroll) = shell.read_with(cx, |shell, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        (39. * pane.row_height(), pane.row_height(), pane.scroll_y)
    });
    assert!(
        scroll <= top && top + height <= scroll + 700.,
        "the asked row is scrolled into view: row at {top}, view from {scroll}"
    );
    let footer = footer_left(&shell, cx);
    assert!(
        footer.starts_with("1 of 76 selected"),
        "the footer counts it: {footer}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn side_button(cx: &mut VisualTestContext, direction: gpui::NavigationDirection) {
    let at = gpui::point(gpui::px(550.), gpui::px(350.));
    cx.simulate_mouse_down(
        at,
        gpui::MouseButton::Navigate(direction),
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        at,
        gpui::MouseButton::Navigate(direction),
        Modifiers::none(),
    );
    cx.run_until_parked();
}

#[gpui::test]
fn the_mouse_side_buttons_go_back_forward_and_up(cx: &mut TestAppContext) {
    use gpui::NavigationDirection::{Back, Forward};
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("enter");
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 2);
    side_button(cx, Back);
    assert_eq!(asked(&shell, cx), "/files", "Back follows the history");
    land(&shell, cx, "/files", home(), 3);
    side_button(cx, Forward);
    assert_eq!(
        asked(&shell, cx),
        "/files/beta",
        "Forward goes again through the Back"
    );
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 4);
    side_button(cx, Back);
    land(&shell, cx, "/files", home(), 5);
    side_button(cx, Back);
    assert_eq!(asked(&shell, cx), "/", "with no history Back climbs");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// EY-312: one side button press navigates once, so no second press is refused with "A directory is already loading.".
#[gpui::test]
fn a_side_button_over_the_listing_says_nothing_while_its_folder_loads(cx: &mut TestAppContext) {
    use gpui::NavigationDirection::{Back, Forward};
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("enter");
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 2);
    side_button(cx, Back);
    assert_eq!(asked(&shell, cx), "/files");
    assert_eq!(
        notice(&shell, cx),
        "",
        "back says nothing while its folder loads"
    );
    land(&shell, cx, "/files", home(), 3);
    side_button(cx, Forward);
    assert_eq!(asked(&shell, cx), "/files/beta");
    assert_eq!(
        notice(&shell, cx),
        "",
        "forward says nothing while its folder loads"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn notice(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.update(cx, |shell, cx| {
        shell.status_messages(cx, |messages, _| messages.notice().to_owned())
    })
}

#[path = "listing_tail_tests.rs"]
mod extracted;

// MouseNav.js refused: no side button navigates behind an open context menu, so it never acts on another folder's rows.
#[gpui::test]
fn noscene_a_side_button_behind_an_open_menu_never_navigates(cx: &mut TestAppContext) {
    use gpui::NavigationDirection::{Back, Forward};
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("enter");
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 2);
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("enter");
    land(&shell, cx, "/files/beta/inner", vec![row("deep", true)], 3);
    side_button(cx, Back);
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 4);
    // Both stacks hold a place now, so each refusal below is the menu's and not an empty history.
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("m");
    super::menus_tests::answer(&shell, cx, &[0o040755]);
    let open_menu = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| shell.menu_opened())
    };
    assert!(open_menu(&shell, cx), "m opens the row menu");
    side_button(cx, Forward);
    assert_eq!(
        asked(&shell, cx),
        "/files/beta",
        "Forward is refused behind the menu"
    );
    assert!(open_menu(&shell, cx), "and the menu is still open");
    side_button(cx, Back);
    assert_eq!(asked(&shell, cx), "/files/beta", "and so is Back");
    cx.simulate_keystrokes("escape");
    assert!(!open_menu(&shell, cx), "escape closes the menu");
    side_button(cx, Back);
    assert_eq!(
        asked(&shell, cx),
        "/files",
        "with the menu closed Back goes back"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
