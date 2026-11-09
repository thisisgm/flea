//! Return, a double click and Enter on an AppImage reach flea --open, whose run ask opens the run card in the window.
use super::test_shell::{land, open_root, row, set_cursor};
use crate::window::u07_tests::{press, read};
use flea::protocol::RowFacts;
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::cell::RefCell;

#[path = "runcard_modifiers_tests.rs"]
mod modifiers;

thread_local! {
    static LAUNCHED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
    static ANSWER: RefCell<(Option<i32>, String)> = const { RefCell::new((Some(0), String::new())) };
    static RUN_LAUNCHED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
    static RUN_ANSWER: RefCell<(Option<i32>, String)> = const { RefCell::new((Some(0), String::new())) };
}

// The test launchers: record the argv and answer what the test set, never starting a process.
fn record(argv: &[String]) -> crate::g2::open_with_properties::lane::Exit {
    LAUNCHED.with(|launched| launched.borrow_mut().push(argv.to_vec()));
    ANSWER.with(|answer| answer.borrow().clone())
}

fn record_run(argv: &[String]) -> crate::g2::open_with_properties::lane::Exit {
    RUN_LAUNCHED.with(|launched| launched.borrow_mut().push(argv.to_vec()));
    RUN_ANSWER.with(|answer| answer.borrow().clone())
}

fn answering(status: Option<i32>, stdout: &str) {
    ANSWER.with(|answer| *answer.borrow_mut() = (status, stdout.into()));
}

fn launched() -> Vec<Vec<String>> {
    LAUNCHED.with(|launched| std::mem::take(&mut *launched.borrow_mut()))
}

fn run_launched() -> Vec<Vec<String>> {
    RUN_LAUNCHED.with(|launched| std::mem::take(&mut *launched.borrow_mut()))
}

const SCRIPT_ASK: &str = r#"{"ask":"run","kind":"script","size":21,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":11,"mtimeNs":12,"target":"/files/hello.sh"}"#;
const APPIMAGE_ASK: &str = r#"{"ask":"run","kind":"appimage","size":99,"handlerId":"","handlerName":"","dev":"8","ino":"10","mtimeS":11,"mtimeNs":12,"target":"/files/tool.AppImage"}"#;
const ELF_ASK: &str = r#"{"ask":"run","kind":"elf","size":99,"handlerId":"","handlerName":"","dev":"8","ino":"10","mtimeS":11,"mtimeNs":12,"target":"/files/tool"}"#;

fn rows() -> Vec<RowFacts> {
    let mut elf = row("tool", false);
    elf.icon = "application-x-executable".into();
    elf.mode = 0o100755;
    vec![
        row("alpha", true),
        row("hello.sh", false),
        row("tool.AppImage", false),
        elf,
    ]
}

fn program_glyph(
    shell: &Entity<crate::state::Shell>,
    cx: &mut VisualTestContext,
) -> Option<crate::g2::open_with_properties::shell::Glyph> {
    let catalog = crate::g2::open_with_properties::tests::english();
    shell.read_with(cx, |shell, _| {
        shell.run_card.as_ref().and_then(|open| {
            open.card
                .card(&catalog)
                .body
                .iter()
                .find_map(|block| match block {
                    crate::g2::open_with_properties::shell::Block::Program { glyph, .. } => {
                        Some(*glyph)
                    }
                    _ => None,
                })
        })
    })
}

// The production Shell on /files through the real Root, with test launchers on the opener and the card.
fn shell(cx: &mut TestAppContext) -> (Entity<crate::state::Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", rows(), 1);
    shell.update(cx, |shell, _| {
        for lane in &mut shell.opener {
            lane.launch = record;
        }
    });
    launched();
    run_launched();
    answering(Some(0), "");
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    (shell, cx)
}

fn card_title(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) -> Option<String> {
    let catalog = crate::g2::open_with_properties::tests::english();
    shell.read_with(cx, |shell, _| {
        shell
            .run_card
            .as_ref()
            .map(|open| open.card.card(&catalog).title)
    })
}

fn card_buttons(
    shell: &Entity<crate::state::Shell>,
    cx: &mut VisualTestContext,
) -> Option<Vec<String>> {
    let catalog = crate::g2::open_with_properties::tests::english();
    shell.read_with(cx, |shell, _| {
        shell.run_card.as_ref().map(|open| {
            open.card
                .card(&catalog)
                .buttons
                .iter()
                .map(|button| button.label.clone())
                .collect()
        })
    })
}

fn quit(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn return_on_an_executable_opens_the_run_card(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    set_cursor(&shell, cx, 1);
    answering(Some(4), SCRIPT_ASK);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        launched()
            .iter()
            .map(|argv| argv[2].clone())
            .collect::<Vec<_>>(),
        ["/files/hello.sh"],
        "Return hands the executable to flea --open"
    );
    assert_eq!(
        card_title(&shell, cx).as_deref(),
        Some("hello.sh is a program.")
    );
    assert_eq!(
        card_buttons(&shell, cx),
        Some(vec![
            "Cancel".into(),
            "Open as Text".into(),
            "Run in Terminal".into()
        ]),
        "the card the board draws"
    );
    assert!(
        cx.debug_bounds("op3-card").is_some(),
        "the run card is drawn in the window"
    );
    assert_eq!(
        program_glyph(&shell, cx),
        Some(crate::g2::open_with_properties::shell::Glyph::Code),
        "a script draws the board's code mark"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn a_binary_opens_with_its_listed_rows_mark(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    set_cursor(&shell, cx, 3);
    answering(Some(4), ELF_ASK);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        card_title(&shell, cx).as_deref(),
        Some("tool is a program.")
    );
    // v0.3.12 RunFile.listedGlyph: a binary keeps the mark its listed row draws, the terminal for an executable.
    assert_eq!(
        program_glyph(&shell, cx),
        Some(crate::g2::open_with_properties::shell::Glyph::Terminal)
    );
    quit(&shell, cx);
}

#[gpui::test]
fn a_double_click_on_an_executable_opens_the_run_card(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    set_cursor(&shell, cx, 1);
    answering(Some(4), SCRIPT_ASK);
    let centre = read(&shell, cx, "rowCentre", &["1"]);
    press(cx, &centre, 2);
    cx.run_until_parked();
    assert_eq!(
        launched()
            .iter()
            .map(|argv| argv[2].clone())
            .collect::<Vec<_>>(),
        ["/files/hello.sh"],
        "a double click hands the executable to flea --open"
    );
    assert_eq!(
        card_title(&shell, cx).as_deref(),
        Some("hello.sh is a program.")
    );
    quit(&shell, cx);
}

#[gpui::test]
fn enter_on_an_appimage_opens_the_run_card_and_escape_closes_it(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    set_cursor(&shell, cx, 2);
    answering(Some(4), APPIMAGE_ASK);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        launched()
            .iter()
            .map(|argv| argv[2].clone())
            .collect::<Vec<_>>(),
        ["/files/tool.AppImage"]
    );
    assert_eq!(
        card_title(&shell, cx).as_deref(),
        Some("tool.AppImage is a program.")
    );
    assert_eq!(
        card_buttons(&shell, cx),
        Some(vec!["Cancel".into(), "Run in Terminal".into()]),
        "a binary offers no Open as Text"
    );
    shell.update(cx, |shell, _| {
        if let Some(open) = shell.run_card.as_mut() {
            open.launch = record_run;
        }
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(card_title(&shell, cx), None, "Escape closes the card");
    assert!(run_launched().is_empty(), "Escape launches nothing");
    quit(&shell, cx);
}

#[gpui::test]
fn enter_on_the_open_run_card_runs_in_a_terminal_and_shift_enter_runs(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    set_cursor(&shell, cx, 1);
    answering(Some(4), SCRIPT_ASK);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        card_title(&shell, cx).as_deref(),
        Some("hello.sh is a program.")
    );
    shell.update(cx, |shell, _| {
        if let Some(open) = shell.run_card.as_mut() {
            open.launch = record_run;
        }
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let ran = run_launched();
    assert_eq!(ran.len(), 1, "Enter on the card launches once: {ran:?}");
    assert_eq!(ran[0][1], "--run-terminal");
    assert_eq!(ran[0][2], "/files/hello.sh");
    assert_eq!(card_title(&shell, cx), None, "the launch closes the card");
    // Shift held on Enter is Run, not Run in Terminal.
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        card_title(&shell, cx).as_deref(),
        Some("hello.sh is a program.")
    );
    shell.update(cx, |shell, _| {
        if let Some(open) = shell.run_card.as_mut() {
            open.launch = record_run;
        }
    });
    cx.simulate_keystrokes("shift-enter");
    cx.run_until_parked();
    let ran = run_launched();
    assert_eq!(
        ran.len(),
        1,
        "Shift+Enter on the card launches once: {ran:?}"
    );
    assert_eq!(ran[0][1], "--run");
    assert_eq!(card_title(&shell, cx), None);
    quit(&shell, cx);
}

// Down in the listing would move the cursor; with the card up the card takes it and the cursor stays.
#[gpui::test]
fn the_run_card_keeps_the_keyboard_while_it_shows(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    set_cursor(&shell, cx, 1);
    answering(Some(4), SCRIPT_ASK);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    launched();
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let cursor = shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor);
    assert_eq!(cursor, 1, "keys belong to the card while it shows");
    assert!(launched().is_empty());
    assert_eq!(
        card_title(&shell, cx).as_deref(),
        Some("hello.sh is a program.")
    );
    quit(&shell, cx);
}

// v0.3.12 RunCard.qml: the scrim's dismiss chooses cancel, so a press beside the card closes it and runs nothing.
#[gpui::test]
fn a_press_on_the_scrim_cancels_the_run_card(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    set_cursor(&shell, cx, 1);
    answering(Some(4), SCRIPT_ASK);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    shell.update(cx, |shell, _| {
        if let Some(open) = shell.run_card.as_mut() {
            open.launch = record_run;
        }
    });
    let scrim = cx
        .debug_bounds("op3-runcard-scrim")
        .expect("the scrim is drawn");
    let card = cx.debug_bounds("op3-card").expect("the card is drawn");
    let at = scrim.origin + gpui::point(gpui::px(4.), gpui::px(4.));
    assert!(!card.contains(&at), "the press lands beside the card");
    // A press on the card's own body, away from its buttons, keeps the card.
    let inside = card.origin + gpui::point(gpui::px(4.), gpui::px(4.));
    cx.simulate_mouse_move(inside, None, gpui::Modifiers::none());
    cx.simulate_mouse_down(inside, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(inside, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        card_title(&shell, cx).as_deref(),
        Some("hello.sh is a program."),
        "a press on the card is not a press on the scrim"
    );
    cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(card_title(&shell, cx), None, "the scrim press cancels");
    assert!(run_launched().is_empty(), "a cancel runs nothing");
    quit(&shell, cx);
}

// A click reads Shift from its own press: Shift+Tab then a plain click runs in a terminal, a Shift click runs.
#[gpui::test]
fn a_click_reads_shift_from_the_click_not_the_last_key(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    set_cursor(&shell, cx, 1);
    answering(Some(4), SCRIPT_ASK);
    for (shifted, flag) in [(false, "--run-terminal"), (true, "--run")] {
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            card_title(&shell, cx).as_deref(),
            Some("hello.sh is a program.")
        );
        shell.update(cx, |shell, _| {
            if let Some(open) = shell.run_card.as_mut() {
                open.launch = record_run;
            }
        });
        cx.simulate_keystrokes("shift-tab");
        cx.run_until_parked();
        let modifiers = if shifted {
            gpui::Modifiers::shift()
        } else {
            gpui::Modifiers::none()
        };
        let button = cx
            .debug_bounds("op3-button-2")
            .expect("Run in Terminal is drawn");
        cx.simulate_click(button.center(), modifiers);
        cx.run_until_parked();
        let ran = run_launched();
        assert_eq!(ran.len(), 1, "the click launches once: {ran:?}");
        assert_eq!(
            ran[0][1], flag,
            "a click with shift {shifted} launches {flag}"
        );
        assert_eq!(card_title(&shell, cx), None);
    }
    quit(&shell, cx);
}
