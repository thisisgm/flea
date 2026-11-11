//! EY-293: Open With's Another Application leaf on the row menu opens the Open With card, which asks its catalogue once its snapshot is held.
use super::menudeliver_tests::{open, select, wire, REG};
use super::menus_tests::{choose, deliver, home, reply, settle, validated};
use super::test_shell::{land, status};
use crate::state::Shell;
use gpui::{Entity, TestAppContext, VisualTestContext};

#[gpui::test]
fn another_application_on_the_row_menu_opens_the_open_with_card(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    select(&shell, cx, &[2]);
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("shift-f10");
    let id = shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().unwrap().id);
    deliver(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[{REG}]}}"#),
    );
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[{{"id":"nvim.desktop","label":"Neovim","icon":"nvim","default":true}}]}}"#
        ),
    );
    // Enter on Open With opens its flyout; Down passes the separator to the tail row.
    choose(&shell, cx, "openWith");
    cx.simulate_keystrokes("down enter");
    settle(cx);
    sent();
    validated(
        &shell,
        cx,
        &format!(
            "openWith:{}",
            flea::ui_logic_context_menus::rows::OPEN_WITH_OTHER
        ),
        &["/files/notes.txt"],
    );
    let lines = sent();
    assert_ne!(
        status(&shell, cx),
        "That action is no longer available; reopen the menu.",
        "the tail row is claimed: {lines:?}"
    );
    let snapshot = lines
        .iter()
        .find(|line| line.contains(r#""op":"snapshot""#))
        .unwrap_or_else(|| panic!("the card snapshots notes.txt: {lines:?}"));
    assert!(
        snapshot.contains(r#""listing":1,"rows":[2],"cursor":2"#),
        "the card's snapshot names its listing: {snapshot}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.contains(r#""op":"applications""#)),
        "the catalogue waits for the snapshot's answer: {lines:?}"
    );
    let card = shell.read_with(cx, |shell, _| shell.open_with.as_ref().map(|open| open.id));
    let card = card.expect("the Open With card opens");
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{card},"op":"snapshot","ok":true,"kinds":[{REG}]}}"#),
    );
    assert_eq!(
        sent(),
        [format!(
            r#"{{"c":"menuaction","op":"applications","id":{card},"installed":true}}"#
        )]
    );
    let title = shell.read_with(cx, |shell, _| {
        let catalog = crate::g2::open_with_properties::tests::english();
        shell
            .open_with
            .as_ref()
            .map(|open| open.card.card(&catalog).title)
    });
    assert_eq!(title.as_deref(), Some("Open notes.txt With"));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

/// Launches Neovim from the row menu's Open With flyout on notes.txt; returns the wire, the menu id and the lines the launch sent.
fn launch(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
) -> (impl FnMut() -> Vec<String>, u64, Vec<String>) {
    land(shell, cx, "/files", home(), 1);
    select(shell, cx, &[2]);
    let mut sent = wire(shell, cx);
    cx.simulate_keystrokes("shift-f10");
    let id = shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().unwrap().id);
    deliver(
        shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[{REG}]}}"#),
    );
    reply(
        shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[{{"id":"nvim.desktop","label":"Neovim","icon":"nvim","default":true}}]}}"#
        ),
    );
    choose(shell, cx, "openWith");
    cx.simulate_keystrokes("enter");
    settle(cx);
    sent();
    validated(shell, cx, "openWith:nvim.desktop", &["/files/notes.txt"]);
    let lines = sent();
    (sent, id, lines)
}

#[gpui::test]
fn an_application_on_the_row_menu_launches_on_the_menus_snapshot(cx: &mut TestAppContext) {
    // SW-03: a flyout application launches through the menu's own snapshot, which stays held until the launch answers.
    let (shell, cx) = open(cx);
    let (mut sent, id, lines) = launch(&shell, cx);
    assert_ne!(
        status(&shell, cx),
        "That action is no longer available; reopen the menu.",
        "the application leaf is claimed: {lines:?}"
    );
    assert!(
        lines.contains(&format!(
            r#"{{"c":"menuaction","op":"openWith","id":{id},"application":"nvim.desktop"}}"#
        )),
        "the launch names the menu's snapshot and the application: {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains(r#""op":"close""#)),
        "a close before the answer would cancel the launch: {lines:?}"
    );
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"openWith","ok":false,"error":"Neovim could not start."}}"#
        ),
    );
    assert_eq!(status(&shell, cx), "Neovim could not start.");
    assert_eq!(
        sent(),
        [format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)],
        "the answer frees the snapshot"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

/// A launch that started, or that a newer menu cancelled, leaves the footer as it was and still frees the snapshot.
fn quiet_answer(cx: &mut TestAppContext, answer: &str) {
    let (shell, cx) = open(cx);
    let (mut sent, id, _) = launch(&shell, cx);
    let before = status(&shell, cx);
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"openWith",{answer}}}"#),
    );
    assert_eq!(status(&shell, cx), before, "{answer}");
    assert_eq!(
        sent(),
        [format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)],
        "{answer}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_launched_application_says_nothing_and_frees_the_snapshot(cx: &mut TestAppContext) {
    quiet_answer(cx, r#""ok":true,"path":"/files/notes.txt""#);
}

#[gpui::test]
fn a_cancelled_launch_says_nothing_and_frees_the_snapshot(cx: &mut TestAppContext) {
    quiet_answer(
        cx,
        r#""ok":false,"error":"Menu request cancelled.","cancelled":true"#,
    );
}

#[gpui::test]
fn a_launch_on_the_other_side_keeps_this_sides_answer(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    let (mut sent, id, _) = launch(&shell, cx);
    // Each side holds its own launch, so the other pane's launch cannot take this side's answer.
    shell.update(cx, |shell, cx| {
        shell.open_with_application(1, id + 1, "nvim.desktop", cx)
    });
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"openWith","ok":false,"error":"Neovim could not start."}}"#
        ),
    );
    assert_eq!(status(&shell, cx), "Neovim could not start.");
    assert_eq!(
        sent(),
        [format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)],
        "this side's answer frees this side's snapshot"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
