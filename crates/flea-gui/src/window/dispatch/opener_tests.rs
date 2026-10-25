//! Enter, l, Right, a double click and a search result on a file run the opener through the production Shell.
use super::opener::Asked;
use super::search_open_tests::answer;
use super::test_shell::{asked, land, open, row, set_cursor, status};
use super::Order;
use crate::window::input::ActionEvent;
use crate::window::u07_tests::{press, read};
use flea::protocol::RowFacts;
use flea::ui_logic_op3::opener::Opened;
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc};

thread_local! {
    static LAUNCHED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
    static ANSWER: RefCell<(Option<i32>, String)> = const { RefCell::new((Some(0), String::new())) };
}

// The test launcher: records the argv and answers what the test set, never starting a process.
fn record(argv: &[String]) -> crate::g2::op3::lane::Exit {
    LAUNCHED.with(|launched| launched.borrow_mut().push(argv.to_vec()));
    ANSWER.with(|answer| answer.borrow().clone())
}

fn answering(status: Option<i32>, stdout: &str) {
    ANSWER.with(|answer| *answer.borrow_mut() = (status, stdout.into()));
}

// The paths flea --open was handed since the last call.
pub(super) fn launched() -> Vec<String> {
    LAUNCHED.with(|launched| std::mem::take(&mut *launched.borrow_mut()))
        .into_iter()
        .map(|argv| {
            assert_eq!(argv[1], "--open", "the opener runs flea --open");
            argv[2..].join(" ")
        })
        .collect()
}

pub(super) fn home() -> Vec<RowFacts> {
    let mut archive = row("backup.tar.zst", false);
    archive.icon = "package-x-generic".into();
    vec![
        row("alpha", true),
        row("notes.txt", false),
        archive,
        row("tool.AppImage", false),
    ]
}

pub(super) type Seen = (Rc<RefCell<Vec<&'static str>>>, Rc<RefCell<Vec<Asked>>>);

// The production Shell on /files with the test launcher on both panes, and what it emits for observers.
pub(super) fn shell(cx: &mut TestAppContext) -> (Entity<crate::state::Shell>, &mut VisualTestContext, Seen, Vec<gpui::Subscription>) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    shell.update(cx, |shell, _| {
        for lane in &mut shell.opener {
            lane.launch = record;
        }
    });
    launched();
    answering(Some(0), "");
    let actions = Rc::new(RefCell::new(Vec::new()));
    let asks = Rc::new(RefCell::new(Vec::new()));
    let (sink, ask_sink) = (actions.clone(), asks.clone());
    let subscriptions = cx.update(|_, cx| {
        vec![
            cx.subscribe(&shell, move |_, event: &ActionEvent, _| sink.borrow_mut().push(event.action)),
            cx.subscribe(&shell, move |_, event: &Asked, _| ask_sink.borrow_mut().push(event.clone())),
        ]
    });
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    (shell, cx, (actions, asks), subscriptions)
}

fn clear_status(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, _| shell.panes[0].as_mut().unwrap().status = Default::default());
}

pub(super) fn quit(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn enter_l_and_a_double_click_on_a_file_hand_it_to_flea_open(cx: &mut TestAppContext) {
    let (shell, cx, (actions, _), _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("enter");
    assert_eq!(launched(), ["/files/notes.txt"], "Enter opens the file");
    assert_eq!(asked(&shell, cx), "/files", "the pane stays where it is");
    let centre = read(&shell, cx, "rowCentre", &["1"]);
    press(cx, &centre, 1);
    assert_eq!(launched(), Vec::<String>::new(), "one click only selects");
    press(cx, &centre, 2);
    assert_eq!(launched(), ["/files/notes.txt"], "a double click opens the file");
    // Nav.js:239: an AppImage is a program, so it goes to the opener and its run card, not Quick Look.
    set_cursor(&shell, cx, 3);
    cx.simulate_keystrokes("enter");
    assert_eq!(launched(), ["/files/tool.AppImage"]);
    assert!(!actions.borrow().contains(&"open"), "no open is handed to nobody");
    quit(&shell, cx);
}

#[gpui::test]
fn right_on_a_file_is_the_preview_key(cx: &mut TestAppContext) {
    let (shell, cx, (actions, _), _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    // Focus.js:139: pageForward on a file row resolves to preview, which runs nothing on flea --open.
    cx.simulate_keystrokes("right");
    assert_eq!(launched(), Vec::<String>::new());
    assert_eq!(asked(&shell, cx), "/files");
    assert_eq!(actions.borrow().as_slice(), ["preview"], "Right hands the preview key on");
    quit(&shell, cx);
}

#[gpui::test]
fn vim_l_opens_a_file(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "vim");
    land(&shell, cx, "/files", home(), 1);
    shell.update(cx, |shell, _| shell.opener[0].launch = record);
    launched();
    answering(Some(0), "");
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("l");
    assert_eq!(launched(), ["/files/notes.txt"], "vim l opens the file");
    quit(&shell, cx);
}

#[gpui::test]
fn enter_on_an_archive_hands_it_to_quick_look(cx: &mut TestAppContext) {
    let (shell, cx, (actions, _), _keep) = shell(cx);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("enter");
    assert_eq!(launched(), Vec::<String>::new(), "an archive never reaches flea --open");
    assert_eq!(actions.borrow().as_slice(), ["preview"]);
    quit(&shell, cx);
}

#[gpui::test]
fn the_exit_status_picks_the_next_step(cx: &mut TestAppContext) {
    let (shell, cx, (_, asks), _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    answering(Some(2), "");
    cx.simulate_keystrokes("enter");
    assert_eq!(status(&shell, cx), "No application on this system opened that file.");
    // An ask status hands its answer to the card host; a run ask without its line is the plain failure.
    answering(Some(4), "{\"ask\":\"run\"}");
    cx.simulate_keystrokes("enter");
    answering(Some(5), "");
    cx.simulate_keystrokes("enter");
    // Each failure press starts from a clear status, so a regression to Done cannot pass on an older sentence.
    for answer in [Some(4), None] {
        clear_status(&shell, cx);
        answering(answer, "");
        cx.simulate_keystrokes("enter");
        assert_eq!(status(&shell, cx), "No application on this system opened that file.", "{answer:?} with no line");
    }
    assert_eq!(launched().len(), 5);
    let path = "/files/notes.txt".to_string();
    assert_eq!(
        asks.borrow().as_slice(),
        [
            Asked { side: 0, opened: Opened::AskRun { path: path.clone(), text: "{\"ask\":\"run\"}".into() } },
            Asked { side: 0, opened: Opened::AskOpenWith(path) },
        ]
    );
    // Opener.qml isDirectoryStatus: the path named a folder by the time it ran, so the pane goes there.
    answering(Some(3), "");
    cx.simulate_keystrokes("enter");
    assert_eq!(launched().len(), 1);
    assert_eq!(asked(&shell, cx), "/files/notes.txt");
    quit(&shell, cx);
}

#[gpui::test]
fn a_second_open_while_one_is_out_is_refused_out_loud(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    let enter = ActionEvent::bare("open", flea::keys::Context::Listing);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.dispatch(Order::Action(enter), window, cx);
            shell.dispatch(Order::Action(enter), window, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(launched(), ["/files/notes.txt"], "only the first press launches");
    assert_eq!(status(&shell, cx), "Still opening the last file; try again in a moment.");
    // Once the first exits the lane is free again.
    cx.simulate_keystrokes("enter");
    assert_eq!(launched(), ["/files/notes.txt"]);
    quit(&shell, cx);
}

#[gpui::test]
fn enter_on_a_search_result_file_opens_that_result(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    cx.simulate_keystrokes("f s");
    answer(&shell, cx, 1, vec![row("sub/deep", true), row("sub/found.txt", false)]);
    cx.simulate_keystrokes("enter down enter");
    assert_eq!(launched(), ["/files/sub/found.txt"], "the result under the cursor opens");
    quit(&shell, cx);
}
