//! SW-03: every refused context menu row, by keyboard and by click, reaches its owner instead of the unavailable notice.
use super::{
    menus_tests::{actions, answer, choose, entry, home, validated},
    test_shell::{land, open_root_with, status},
};
use crate::{backend::Backend, state::Shell, window::u07_tests::read};
use flea::{jsondoc::Json, protocol::RowFacts};
use gpui::{
    point, px, Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext,
    VisualTestContext,
};

const UNAVAILABLE: &str = "That action is no longer available; reopen the menu.";
const DIR: u32 = 0o040755;
const FILE: u32 = 0o100644;
pub(super) const REG: u32 = 32768;
const PAIR: &str = r#""a":"/files/notes.txt","b":"/files/todo.md""#;

pub(super) fn open(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    // Delete and Permissions shown, Run Script hidden, so the menu asks only for snapshot and applications.
    open_root_with(
        cx,
        r#""keys":"default","view":"list","menu":{"hidden":["runScript"]}"#,
        None,
    )
}

pub(super) fn select(shell: &Entity<Shell>, cx: &mut VisualTestContext, rows: &[usize]) {
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = rows.to_vec();
        host.pane.cursor = *rows.last().unwrap();
    });
}

/// The pane's backend becomes a line recorder; every request the window sends is read back.
pub(super) fn wire(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
) -> impl FnMut() -> Vec<String> {
    let (backend, mut read) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    move || std::iter::from_fn(&mut read).collect()
}

pub(super) fn open_menu(shell: &Entity<Shell>, cx: &mut VisualTestContext, kinds: &[u32]) {
    cx.simulate_keystrokes("shift-f10");
    answer(shell, cx, kinds);
}

/// Scrolls the action into view, then clicks its drawn row through the real pointer route.
fn click(shell: &Entity<Shell>, cx: &mut VisualTestContext, action: &str) {
    let index = actions(shell, cx)
        .iter()
        .position(|row| row == action)
        .unwrap_or_else(|| panic!("the menu offers {action}: {:?}", actions(shell, cx)));
    for _ in 0..=index {
        let (row, wheel) = shell.read_with(cx, |shell, _| {
            let menu = shell.menu.menu.borrow();
            let card = menu.card.as_ref().expect("a drawn card");
            let x = card.frame.x + 24.;
            let y = card.row_screen_top(index)
                + card.built.metrics.height(&card.entries[index]) as f32 / 2.;
            (
                (card.row_at(x, y) == Some(index)).then_some((x, y)),
                point(px(x), px(card.frame.y + card.frame.h / 2.)),
            )
        });
        if let Some((x, y)) = row {
            press_at(cx, x, y);
            super::menus_tests::settle(cx);
            return;
        }
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: wheel,
            delta: gpui::ScrollDelta::Lines(point(0., -3.)),
            modifiers: Modifiers::none(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        super::menus_tests::settle(cx);
    }
    panic!("the menu did not scroll {action} into view");
}

/// One left press and release at this window position.
fn press_at(cx: &mut VisualTestContext, x: f32, y: f32) {
    let position = point(px(x), px(y));
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
}

fn favourites(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<String> {
    shell.read_with(cx, |shell, _| {
        shell
            .launch
            .settings
            .data
            .get("places")
            .and_then(|places| places.get("favourites"))
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|row| row.get("path").and_then(Json::as_str))
            .map(str::to_owned)
            .collect()
    })
}

fn pair(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    select(&shell, cx, &[2, 3]);
    (shell, cx)
}

#[gpui::test]
fn compare_on_two_files_opens_the_compare_card(cx: &mut TestAppContext) {
    let (shell, cx) = pair(cx);
    let mut sent = wire(&shell, cx);
    open_menu(&shell, cx, &[REG, REG]);
    choose(&shell, cx, "compare");
    validated(
        &shell,
        cx,
        "compare",
        &["/files/notes.txt", "/files/todo.md"],
    );
    let lines = sent();
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""c":"compare""#) && line.contains(PAIR)),
        "the diff is asked of the validated pair: {lines:?}"
    );
    assert_eq!(read(&shell, cx, "previewKind", &[]), "compare");
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn compare_on_two_files_opens_the_compare_card_by_click(cx: &mut TestAppContext) {
    let (shell, cx) = pair(cx);
    let mut sent = wire(&shell, cx);
    open_menu(&shell, cx, &[REG, REG]);
    click(&shell, cx, "compare");
    validated(
        &shell,
        cx,
        "compare",
        &["/files/notes.txt", "/files/todo.md"],
    );
    let lines = sent();
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""c":"compare""#) && line.contains(PAIR)),
        "the diff is asked of the validated pair: {lines:?}"
    );
    assert_eq!(read(&shell, cx, "previewKind", &[]), "compare");
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn duplicate_runs_on_the_menu_row(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    select(&shell, cx, &[2]);
    let mut sent = wire(&shell, cx);
    open_menu(&shell, cx, &[REG]);
    choose(&shell, cx, "duplicate");
    validated(&shell, cx, "duplicate", &["/files/notes.txt"]);
    let lines = sent();
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""c":"duplicate""#) && line.contains("/files/notes.txt")),
        "the copy is made: {lines:?}"
    );
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn rename_opens_the_editor_on_the_menu_row(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    select(&shell, cx, &[2]);
    open_menu(&shell, cx, &[REG]);
    choose(&shell, cx, "rename");
    validated(&shell, cx, "rename", &["/files/notes.txt"]);
    let editing = shell.read_with(cx, |shell, _| shell.rename.edit.is_some());
    assert!(editing, "the editor opens on the chosen row");
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn move_to_trash_runs_on_the_menu_row(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    select(&shell, cx, &[2]);
    let mut sent = wire(&shell, cx);
    open_menu(&shell, cx, &[REG]);
    choose(&shell, cx, "trash");
    validated(&shell, cx, "trash", &["/files/notes.txt"]);
    let lines = sent();
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""c":"trash""#) && line.contains("[2]")),
        "the row is trashed: {lines:?}"
    );
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn move_to_trash_runs_on_the_menu_row_by_click(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    select(&shell, cx, &[2]);
    let mut sent = wire(&shell, cx);
    open_menu(&shell, cx, &[REG]);
    click(&shell, cx, "trash");
    validated(&shell, cx, "trash", &["/files/notes.txt"]);
    let lines = sent();
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""c":"trash""#) && line.contains("[2]")),
        "the row is trashed: {lines:?}"
    );
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn properties_opens_its_card_on_the_menu_row(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    select(&shell, cx, &[2]);
    let mut sent = wire(&shell, cx);
    open_menu(&shell, cx, &[REG]);
    choose(&shell, cx, "properties");
    // The menu's own snapshot carries the same rows, so only what the validated choice sends is the card's.
    sent();
    validated(&shell, cx, "properties", &["/files/notes.txt"]);
    let lines = sent();
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""op":"snapshot""#) && line.contains(r#""listing":1"#)),
        "the card snapshots the rows it asks about under their listing: {lines:?}"
    );
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let card = cx
        .debug_bounds("op3-card")
        .expect("the Properties card is drawn");
    let inside = card.center();
    press_at(cx, f32::from(inside.x), f32::from(inside.y));
    cx.run_until_parked();
    assert!(
        shell.read_with(cx, |shell, _| shell.properties.is_some()),
        "a press on the card keeps it up"
    );
    let outside = (f32::from(card.origin.x) / 2., f32::from(card.origin.y) / 2.);
    assert!(
        outside.0 > 0. && outside.1 > 0.,
        "the scrim shows around the card: {card:?}"
    );
    press_at(cx, outside.0, outside.1);
    cx.run_until_parked();
    assert!(
        shell.read_with(cx, |shell, _| shell.properties.is_none()),
        "a press on the scrim cancels the card"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

/// The actions one context's drawn, enabled, leaf-less rows offer, read off the painted model.
fn drawn_actions(cx: &mut TestAppContext, rows: &[usize], kinds: &[u32]) -> Vec<String> {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    select(&shell, cx, rows);
    open_menu(&shell, cx, kinds);
    let model = flea::jsondoc::parse(&read(&shell, cx, "contextMenuModel", &[])).unwrap();
    let out = model
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| {
            let action = row.get("action").and_then(Json::as_str)?;
            let disabled = row.get("disabled").and_then(Json::as_bool) == Some(true);
            let flyout = row.get("submenu").is_some();
            (!disabled && !flyout && !action.is_empty()).then(|| action.to_owned())
        })
        .collect();
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    out
}

/// Chooses one drawn row in a fresh window for its context and returns the footer afterwards.
fn delivered(
    cx: &mut TestAppContext,
    rows: &[usize],
    kinds: &[u32],
    action: &str,
    paths: &[&str],
) -> String {
    let (shell, cx) = open(cx);
    land(&shell, cx, "/files", home(), 1);
    select(&shell, cx, rows);
    open_menu(&shell, cx, kinds);
    choose(&shell, cx, action);
    validated(&shell, cx, action, paths);
    let said = status(&shell, cx);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    said
}

// Permissions gets its owner from the designed card host on gm/0400-m9-permissions-card (0b9a44c2f); drop it here when that joins.
const OWNER_ELSEWHERE: &[&str] = &["permissions"];

// The table check: every row these menus draw enabled must reach an owner, never the refusal.
#[gpui::test]
fn every_drawn_enabled_row_reaches_an_owner(cx: &mut TestAppContext) {
    let contexts: [(&[usize], &[u32], &[&str]); 3] = [
        (&[2], &[REG], &["/files/notes.txt"]),
        (&[0], &[0o040000], &["/files/alpha"]),
        (
            &[2, 3],
            &[REG, REG],
            &["/files/notes.txt", "/files/todo.md"],
        ),
    ];
    let mut refused = Vec::new();
    for (rows, kinds, paths) in contexts {
        let drawn = drawn_actions(cx, rows, kinds);
        assert!(drawn.len() >= 5, "a full menu is drawn: {drawn:?}");
        for action in drawn {
            if OWNER_ELSEWHERE.contains(&action.as_str()) {
                continue;
            }
            if delivered(cx, rows, kinds, &action, paths) == UNAVAILABLE {
                refused.push(action);
            }
        }
    }
    assert!(
        refused.is_empty(),
        "drawn, enabled rows refused as unavailable: {refused:?}"
    );
}

#[gpui::test]
fn add_to_favourites_adds_the_folder_on_the_menu_row(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    let rows: Vec<RowFacts> = vec![entry("alpha", DIR), entry("notes.txt", FILE)];
    land(&shell, cx, "/files", rows, 1);
    select(&shell, cx, &[0]);
    open_menu(&shell, cx, &[0o040000]);
    choose(&shell, cx, "addFavourite");
    validated(&shell, cx, "addFavourite", &["/files/alpha"]);
    assert_eq!(favourites(&shell, cx), ["/files/alpha"]);
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
