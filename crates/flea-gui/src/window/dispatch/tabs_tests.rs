//! Every tab key, Ctrl+Return, a middle click on a folder row and on a rail place run the strip on the production Shell.
use super::{
    effect_tests::{close, hit, strokes},
    test_shell::{asked, land, open, open_in, open_places, row, set_cursor},
};
use crate::{state::Shell, window::u07_tests::read};
use flea::keys::{self, Context};
use gpui::{
    point, px, Entity, KeyDownEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent,
    TestAppContext, VisualTestContext,
};

fn home() -> Vec<flea::protocol::RowFacts> {
    vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ]
}

// tabCount, tabIndex and tabLabels as the native sweep reads them.
fn strip(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (String, String, String) {
    (
        read(shell, cx, "tabCount", &[]),
        read(shell, cx, "tabIndex", &[]),
        read(shell, cx, "tabLabels", &[]),
    )
}

fn message(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    read(shell, cx, "lastMessage", &[])
}

// /files, then /files/alpha in a second tab, which is current.
fn two_tabs<'a>(
    cx: &'a mut TestAppContext,
    preset: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = open(cx, preset);
    land(&shell, cx, "/files", home(), 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_tab(0, "/files/alpha".into(), window, cx)
        })
    });
    land(&shell, cx, "/files/alpha", vec![row("inner", true)], 2);
    assert_eq!(
        strip(&shell, cx),
        ("2".into(), "1".into(), "files|alpha".into())
    );
    (shell, cx)
}

fn middle(cx: &mut VisualTestContext, centre: &str) {
    let mut values = centre
        .split_whitespace()
        .map(|value| value.parse::<f32>().unwrap());
    let position = point(
        px(values.next().expect("drawn x")),
        px(values.next().expect("drawn y")),
    );
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Middle,
        click_count: 1,
        modifiers: Modifiers::none(),
        first_mouse: false,
    });
    cx.simulate_mouse_up(position, MouseButton::Middle, Modifiers::none());
    cx.run_until_parked();
}

// From two_tabs, a third tab with the second current again, so next and previous land on different tabs.
fn middle_of_three(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_tab(0, "/files/beta".into(), window, cx)
        })
    });
    land(shell, cx, "/files/beta", Vec::new(), 3);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.tab_action("tabPrevious", window, cx))
        })
    });
    land(shell, cx, "/files/alpha", vec![row("inner", true)], 4);
    assert_eq!(
        strip(shell, cx),
        ("3".into(), "1".into(), "files|alpha|beta".into())
    );
}

// The presets that bind no key for a strip action in a context the sweep visits.
const UNBOUND: &[(&str, &str, Context)] = &[];

// What each strip key leaves from its start: tab count, current index, labels, and the folder asked for.
fn expected(action: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match action {
        "tabNew" => ("3", "2", "files|alpha|alpha", "/files/alpha"),
        "openTab" => ("3", "2", "files|alpha|inner", "/files/alpha/inner"),
        "tabClose" => ("1", "0", "files", "/files"),
        "tabNext" => ("3", "2", "files|alpha|beta", "/files/beta"),
        "tabPrevious" => ("3", "0", "files|alpha|beta", "/files"),
        "tabMoveLeft" => ("2", "0", "alpha|files", "/files/alpha"),
        "tabMoveRight" => ("2", "1", "files|alpha", "/files/alpha"),
        _ => unreachable!(),
    }
}

#[gpui::test]
fn every_strip_binding_runs_the_tab_model_on_the_production_shell(cx: &mut TestAppContext) {
    let actions = [
        "tabNew",
        "openTab",
        "tabClose",
        "tabNext",
        "tabPrevious",
        "tabMoveLeft",
        "tabMoveRight",
    ];
    for preset in keys::PRESETS {
        for action in actions {
            let rail = matches!(action, "tabNew" | "tabClose" | "tabNext" | "tabPrevious");
            for context in [Context::Listing, Context::Rail] {
                if context == Context::Rail && !rail {
                    continue;
                }
                let events = strokes(preset, action, context);
                assert_eq!(
                    events.is_empty(),
                    UNBOUND.contains(&(preset, action, context)),
                    "{preset} {context:?} {action}: bound keys and the skip list disagree"
                );
                let cycle = matches!(action, "tabNext" | "tabPrevious");
                for event in events {
                    let (shell, cx) = two_tabs(cx, preset);
                    if cycle {
                        middle_of_three(&shell, cx);
                    }
                    // Tabs.js moveCurrent clamps at the end, so a right move starts one place in.
                    if action == "tabMoveRight" {
                        cx.update(|window, cx| {
                            shell.update(cx, |shell, cx| {
                                assert!(shell.tab_action("tabMoveLeft", window, cx))
                            })
                        });
                    }
                    if context == Context::Rail {
                        cx.update(|window, cx| {
                            shell.update(cx, |shell, cx| window.focus(&shell.focus, cx))
                        });
                        cx.simulate_keystrokes("tab");
                        assert_eq!(
                            shell.read_with(cx, |shell, _| shell.keyboard.context),
                            Context::Rail
                        );
                    }
                    hit(cx, &event, action);
                    let (count, index, labels, path) = expected(action);
                    let at = format!("{preset} {context:?} {:?} {action}", event.keystroke);
                    assert_eq!(asked(&shell, cx), path, "{at}");
                    // The current tab draws its landed folder, so a folder asked for lands first.
                    if path != "/files/alpha" {
                        land(&shell, cx, path, home(), if cycle { 5 } else { 3 });
                    }
                    assert_eq!(
                        strip(&shell, cx),
                        (count.into(), index.into(), labels.into()),
                        "{at}"
                    );
                    close(&shell, cx);
                }
            }
        }
    }
}

fn digit(cx: &mut VisualTestContext, text: &str) {
    key_at(cx, text, None);
}

// A press as the fork reports it, with `us` the US key at the same position.
fn key_at(cx: &mut VisualTestContext, text: &str, us: Option<&str>) {
    cx.simulate_event(KeyDownEvent {
        keystroke: Keystroke {
            key: text.into(),
            key_char: Some(text.into()),
            modifiers: Modifiers::none(),
            is_keypad: false,
            us_key: us.map(Into::into),
        },
        is_held: false,
        prefer_character_input: false,
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_digit_selects_its_tab_and_a_missing_tab_is_named_until_the_count_changes(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = two_tabs(cx, "default");
    digit(cx, "1");
    assert_eq!(strip(&shell, cx).1, "0");
    assert_eq!(
        asked(&shell, cx),
        "/files",
        "1 shows the first tab's folder"
    );
    land(&shell, cx, "/files", home(), 3);
    digit(cx, "2");
    assert_eq!(strip(&shell, cx).1, "1");
    assert_eq!(asked(&shell, cx), "/files/alpha");
    land(&shell, cx, "/files/alpha", vec![row("inner", true)], 4);
    for number in 3..=9 {
        digit(cx, &number.to_string());
        assert_eq!(message(&shell, cx), format!("No tab {number}"));
        assert_eq!(strip(&shell, cx).1, "1", "a missing tab moves nothing");
    }
    // Tabs.js dropCount: the count verdict goes when the count changes.
    cx.simulate_keystrokes("t");
    assert_eq!(strip(&shell, cx).0, "3");
    assert_eq!(message(&shell, cx), "");
    // AZERTY types & at the US 1 key, and the window's dispatch still selects the first tab.
    key_at(cx, "&", Some("1"));
    assert_eq!(strip(&shell, cx).1, "0");
    close(&shell, cx);
}

// The native sweep's grid rows: Ctrl+Return and Ctrl+KP_Enter on a file, w and Ctrl+W on the last tab.
#[gpui::test]
fn ctrl_return_on_a_file_and_closing_the_last_tab_say_why_in_list_and_grid(
    cx: &mut TestAppContext,
) {
    let open_tab = strokes("default", "openTab", Context::Listing);
    let close_tab = strokes("default", "tabClose", Context::Listing);
    assert!(open_tab.iter().any(|event| event.keystroke.is_keypad));
    assert!(close_tab
        .iter()
        .any(|event| event.keystroke.key == "w" && !event.keystroke.modifiers.control));
    for view in ["list", "grid"] {
        for (events, action, said) in [
            (&open_tab, "openTab", "Only a folder opens in a new tab."),
            (&close_tab, "tabClose", "The last tab cannot be closed"),
        ] {
            for event in events {
                let (shell, cx) = open_in(cx, "default", view);
                land(&shell, cx, "/files", home(), 1);
                set_cursor(&shell, cx, 2);
                assert_eq!(read(&shell, cx, "tabCount", &[]), "1");
                hit(cx, event, action);
                let at = format!("{view} {:?}", event.keystroke);
                assert_eq!(message(&shell, cx), said, "{at}");
                assert_eq!(read(&shell, cx, "tabCount", &[]), "1", "{at}");
                assert_eq!(asked(&shell, cx), "/files", "{at}");
                close(&shell, cx);
            }
        }
    }
}

#[gpui::test]
fn a_middle_click_opens_a_folder_row_in_a_new_tab_and_a_file_row_does_nothing(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let file = read(&shell, cx, "rowCentre", &["2"]);
    middle(cx, &file);
    assert_eq!(read(&shell, cx, "tabCount", &[]), "1");
    assert_eq!(
        message(&shell, cx),
        "",
        "Tap.js: a file has no folder to open"
    );
    let folder = read(&shell, cx, "rowCentre", &["1"]);
    middle(cx, &folder);
    assert_eq!(asked(&shell, cx), "/files/beta");
    land(&shell, cx, "/files/beta", Vec::new(), 2);
    assert_eq!(
        strip(&shell, cx),
        ("2".into(), "1".into(), "files|beta".into())
    );
    close(&shell, cx);
}

// The drawn centre of the rail row with this label.
fn rail_centre(shell: &Entity<Shell>, cx: &mut VisualTestContext, name: &str) -> String {
    let labels = read(shell, cx, "railLabels", &[]);
    let index = labels
        .split('|')
        .position(|label| label == name)
        .unwrap_or_else(|| panic!("the rail draws {name}: {labels}"))
        .to_string();
    read(shell, cx, "railRowCentre", &[&index])
}

#[gpui::test]
fn a_middle_click_on_home_opens_a_tab_and_on_trash_does_nothing(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let trash = rail_centre(&shell, cx, "Trash");
    middle(cx, &trash);
    assert_eq!(
        read(&shell, cx, "tabCount", &[]),
        "1",
        "PlaceMenu.js: Trash is not a tab"
    );
    assert_eq!(asked(&shell, cx), "/files");
    let centre = rail_centre(&shell, cx, "Home");
    middle(cx, &centre);
    assert_eq!(read(&shell, cx, "tabCount", &[]), "2");
    assert_eq!(read(&shell, cx, "tabIndex", &[]), "1");
    assert_eq!(
        asked(&shell, cx),
        std::env::var("HOME").unwrap_or_else(|_| "/".into())
    );
    close(&shell, cx);
}

// Places.js openEntry: a bad record says why as an error, a remote place opens only here, a file URL opens decoded.
#[gpui::test]
fn a_middle_click_on_a_favourite_refuses_a_bad_or_remote_record_and_decodes_a_file_url(
    cx: &mut TestAppContext,
) {
    let places = r#","favourites":[{"label":"Bad","path":"relative"},{"label":"Share","path":"smb://host/share"},{"label":"Space","path":"file:///tmp/a%20b"}]"#;
    for (name, said, error) in [
        (
            "Bad",
            "Could not open Bad · use a full path, a path that starts with ~/, or a network address",
            "true",
        ),
        ("Share", "Share opens in this tab only", "false"),
    ] {
        let (shell, cx) = open_places(cx, "default", "list", places);
        land(&shell, cx, "/files", home(), 1);
        let centre = rail_centre(&shell, cx, name);
        middle(cx, &centre);
        assert_eq!(message(&shell, cx), said, "{name}");
        assert_eq!(read(&shell, cx, "statusError", &[]), error, "{name}");
        assert_eq!(read(&shell, cx, "tabCount", &[]), "1", "{name}");
        assert_eq!(asked(&shell, cx), "/files", "{name}");
        close(&shell, cx);
    }
    let (shell, cx) = open_places(cx, "default", "list", places);
    land(&shell, cx, "/files", home(), 1);
    let centre = rail_centre(&shell, cx, "Space");
    middle(cx, &centre);
    assert_eq!(read(&shell, cx, "tabCount", &[]), "2");
    assert_eq!(asked(&shell, cx), "/tmp/a b");
    close(&shell, cx);
}

// TabBar.qml open is tabCount > 1 in every view, so Dual draws the focused pane's strip and names every tab.
#[gpui::test]
fn the_dual_view_draws_and_names_every_tab(cx: &mut TestAppContext) {
    let (shell, cx) = open_in(cx, "default", "dual");
    land(&shell, cx, "/files", home(), 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_tab(0, "/files/alpha".into(), window, cx)
        })
    });
    land(&shell, cx, "/files/alpha", vec![row("inner", true)], 2);
    assert_eq!(read(&shell, cx, "tabBarVisible", &[]), "true");
    assert_eq!(
        strip(&shell, cx),
        ("2".into(), "1".into(), "files|alpha".into())
    );
    close(&shell, cx);
}

#[path = "tab_view_tests.rs"]
mod tab_view_tests;
