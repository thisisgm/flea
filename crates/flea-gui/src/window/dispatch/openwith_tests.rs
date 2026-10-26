//! Enter on a file no application claims reaches flea --open, whose Open With ask opens the Open With card in the window (EY-84).
use super::test_shell::{land, open_root, row, sent, set_cursor, status};
use crate::backend::Event as BackendEvent;
use flea::protocol::RowFacts;
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::cell::RefCell;

thread_local! {
    static LAUNCHED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
    static ANSWER: RefCell<(Option<i32>, String)> = const { RefCell::new((Some(0), String::new())) };
}

fn record(argv: &[String]) -> crate::g2::op3::lane::Exit {
    LAUNCHED.with(|launched| launched.borrow_mut().push(argv.to_vec()));
    ANSWER.with(|answer| answer.borrow().clone())
}

fn answering(status: Option<i32>, stdout: &str) {
    ANSWER.with(|answer| *answer.borrow_mut() = (status, stdout.into()));
}

fn launched() -> Vec<Vec<String>> {
    LAUNCHED.with(|launched| std::mem::take(&mut *launched.borrow_mut()))
}

fn rows() -> Vec<RowFacts> {
    vec![row("alpha", true), row("mystery.xyz", false)]
}

// The production Shell on /files through the real Root, with the test launcher on the opener.
fn shell(cx: &mut TestAppContext) -> (Entity<crate::state::Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", rows(), 1);
    shell.update(cx, |shell, _| {
        for lane in &mut shell.opener {
            lane.launch = record;
        }
    });
    launched();
    sent();
    answering(Some(0), "");
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    (shell, cx)
}

fn deliver(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.receive(0, BackendEvent::Producer(line.into()), window, cx))
    });
    cx.run_until_parked();
}

fn card_title(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) -> Option<String> {
    let catalog = crate::g2::op3::tests::english();
    shell.read_with(cx, |shell, _| {
        shell.open_with.as_ref().map(|open| open.card.card(&catalog).title)
    })
}

fn quit(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// The card opens on mystery.xyz and asks the backend for its snapshot and then its applications, under one id.
fn opened(cx: &mut TestAppContext) -> (Entity<crate::state::Shell>, &mut VisualTestContext, u64) {
    let (shell, cx) = shell(cx);
    set_cursor(&shell, cx, 1);
    answering(Some(5), "");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        launched().iter().map(|argv| argv[2].clone()).collect::<Vec<_>>(),
        ["/files/mystery.xyz"],
        "Enter hands the unclaimed file to flea --open"
    );
    let lines = sent();
    let id_of = |op: &str| {
        lines
            .iter()
            .filter_map(|line| flea::jsondoc::parse(line).ok())
            .find(|doc| doc.get("op").and_then(flea::jsondoc::Json::as_str) == Some(op))
            .and_then(|doc| doc.get("id").and_then(flea::jsondoc::Json::as_f64))
            .map(|id| id as u64)
    };
    let snapshot = id_of("snapshot").expect("the card snapshots its row first");
    assert_eq!(id_of("applications"), Some(snapshot), "the applications ask carries the snapshot's id");
    assert!(
        lines.iter().any(|line| line.contains(r#""installed":true"#)),
        "the card asks for the whole catalogue: {lines:?}"
    );
    (shell, cx, snapshot)
}

fn answer(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext, id: u64) {
    deliver(shell, cx, &format!(
        r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"path":"/files/mystery.xyz","kind":"Data","mime":"application/x-mystery","applications":[{{"id":"nvim.desktop","label":"Neovim","icon":"nvim","default":true}}],"installed":[{{"id":"btop.desktop","label":"btop++","icon":"btop","default":false}}]}}"#
    ));
}

#[gpui::test]
fn enter_on_an_unclaimed_file_opens_the_open_with_card(cx: &mut TestAppContext) {
    let (shell, cx, id) = opened(cx);
    assert_eq!(card_title(&shell, cx).as_deref(), Some("Open mystery.xyz With"));
    assert!(cx.debug_bounds("op3-card").is_some(), "the Open With card is drawn in the window");
    answer(&shell, cx, id);
    let catalog = crate::g2::op3::tests::english();
    let labels = shell.read_with(cx, |shell, _| {
        shell.open_with.as_ref().map(|open| {
            open.card
                .card(&catalog)
                .body
                .iter()
                .filter_map(|block| match block {
                    crate::g2::op3::shell::Block::Apps(rows, _) => Some(
                        rows.iter()
                            .filter_map(|row| match row {
                                crate::g2::op3::shell::AppRow::App { label, .. } => Some(label.clone()),
                                _ => None,
                            })
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                })
                .next()
        })
    });
    assert_eq!(labels, Some(Some(vec!["Neovim".into(), "btop++".into()])), "the reply fills the card's list");
    quit(&shell, cx);
}

#[gpui::test]
fn the_open_with_card_opens_the_cursor_app_and_closes_on_the_backends_yes(cx: &mut TestAppContext) {
    let (shell, cx, id) = opened(cx);
    answer(&shell, cx, id);
    sent();
    // Search, list, the always box, Cancel, Open: the board's Tab walk, with Space ticking the box.
    cx.simulate_keystrokes("tab tab space tab tab enter");
    cx.run_until_parked();
    assert_eq!(
        sent(),
        [format!(r#"{{"c":"menuaction","op":"openWith","id":{id},"application":"nvim.desktop","always":true}}"#)],
        "Enter on Open asks for the cursor application with the box ticked"
    );
    assert_eq!(card_title(&shell, cx).as_deref(), Some("Open mystery.xyz With"), "the card waits for the backend");
    deliver(&shell, cx, &format!(r#"{{"t":"menuaction","id":{id},"op":"openWith","ok":true}}"#));
    assert_eq!(card_title(&shell, cx), None, "the backend's yes closes the card");
    quit(&shell, cx);
}

#[gpui::test]
fn a_path_no_longer_listed_says_so_and_opens_nothing(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    // v0.3.12 Pane.qml showOpenWith: a path its listing no longer names is refused with this sentence.
    shell.update(cx, |shell, cx| shell.open_open_with(0, "/files/gone.xyz".into(), cx));
    cx.run_until_parked();
    assert_eq!(card_title(&shell, cx), None, "no card opens for a row that is gone");
    assert_eq!(status(&shell, cx), "That file is no longer listed.");
    quit(&shell, cx);
}

#[gpui::test]
fn escape_on_the_open_with_card_closes_it_and_expires_its_snapshot(cx: &mut TestAppContext) {
    let (shell, cx, id) = opened(cx);
    sent();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(card_title(&shell, cx), None, "Escape closes the card");
    assert_eq!(
        sent(),
        [format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)],
        "Escape sends the close the backend reads"
    );
    quit(&shell, cx);
}

// A second ask while a card shows replaces it, and the replaced Open With card expires its snapshot first.
#[gpui::test]
fn a_second_ask_closes_the_card_that_shows(cx: &mut TestAppContext) {
    let (shell, cx, first) = opened(cx);
    sent();
    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.open_open_with(0, "/files/mystery.xyz".into(), cx)));
    cx.run_until_parked();
    let lines = sent();
    assert_eq!(
        lines.first().cloned(),
        Some(format!(r#"{{"c":"menuaction","op":"close","id":{first}}}"#)),
        "the first card's snapshot expires before the second opens: {lines:?}"
    );
    let second = shell.read_with(cx, |shell, _| shell.open_with.as_ref().map(|open| open.id));
    assert!(second.is_some_and(|id| id != first), "the second ask holds its own id");
    let ask = r#"{"ask":"run","kind":"script","size":21,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":11,"mtimeNs":12,"target":"/files/mystery.xyz"}"#;
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| shell.open_run_card(0, "/files/mystery.xyz".into(), ask.into(), cx))
    });
    cx.run_until_parked();
    assert_eq!(
        sent(),
        [format!(r#"{{"c":"menuaction","op":"close","id":{}}}"#, second.unwrap())],
        "a run ask closes the Open With card"
    );
    assert_eq!(card_title(&shell, cx), None);
    assert!(shell.read_with(cx, |shell, _| shell.run_card.is_some()), "the run card opens in its place");
    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.open_open_with(0, "/files/mystery.xyz".into(), cx)));
    cx.run_until_parked();
    assert!(shell.read_with(cx, |shell, _| shell.run_card.is_none()), "an Open With ask closes the run card");
    quit(&shell, cx);
}

// v0.3.12 DialogScrim: a press beside the card cancels it, as Escape does, and expires the snapshot.
#[gpui::test]
fn a_press_on_the_scrim_cancels_the_open_with_card(cx: &mut TestAppContext) {
    let (shell, cx, id) = opened(cx);
    sent();
    let scrim = cx.debug_bounds("op3-openwith-scrim").expect("the scrim is drawn");
    let card = cx.debug_bounds("op3-card").expect("the card is drawn");
    let at = scrim.origin + gpui::point(gpui::px(4.), gpui::px(4.));
    assert!(!card.contains(&at), "the press lands beside the card");
    // A press on the card's own body, away from its buttons, keeps the card.
    let inside = card.origin + gpui::point(gpui::px(4.), gpui::px(4.));
    cx.simulate_mouse_move(inside, None, gpui::Modifiers::none());
    cx.simulate_mouse_down(inside, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(inside, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(card_title(&shell, cx).as_deref(), Some("Open mystery.xyz With"), "a press on the card is not a press on the scrim");
    cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(card_title(&shell, cx), None, "the scrim press cancels");
    assert_eq!(sent(), [format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)]);
    quit(&shell, cx);
}

// A snapshot the backend never took opens no card, since its applications could never arrive.
#[gpui::test]
fn an_ask_on_a_stopped_backend_opens_no_card(cx: &mut TestAppContext) {
    let (shell, cx) = shell(cx);
    shell.update(cx, |shell, _| shell.panes[0].as_mut().unwrap().backend = None);
    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.open_open_with(0, "/files/mystery.xyz".into(), cx)));
    cx.run_until_parked();
    assert_eq!(card_title(&shell, cx), None, "no card waits on a backend that is gone");
    quit(&shell, cx);
}
