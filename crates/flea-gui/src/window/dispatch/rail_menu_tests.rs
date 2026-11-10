//! Mounted rail menu input and captured-place actions.
use super::menus_tests::{choose, home, settle};
use super::test_shell::{land, open_root_with};
use crate::{state::Shell, window::u07_tests::read};
use flea::{jsondoc, ui_logic_rail_places::rail::Kind};
use gpui::{
    point, px, Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext,
    VisualTestContext,
};

#[path = "trash_rail_menu_tests.rs"]
mod trash;

pub(super) fn open(cx: &mut TestAppContext, full: bool) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_root_with(
        cx,
        if full {
            r#""keys":"default","view":"list","menu":{"hidden":[]}"#
        } else {
            r#""keys":"default","view":"list""#
        },
        None,
    );
    land(&shell, cx, "/files", home(), 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let mut state = shell.launch.settings.data.clone();
            crate::g2::settings_pages::seams::merge(
                &mut state,
                &jsondoc::parse(r#"{"places":{"favourites":[{"label":"Docs","path":"/docs"}]}}"#)
                    .unwrap(),
            );
            shell.apply_settings(state, window, cx);
            let producer = shell.rail.as_ref().unwrap().places.clone();
            producer.update(cx, |producer, cx| {
                producer
                    .places
                    .update(cx, |places, _| places.exe = "/bin/false".into());
                producer.fixture_home("/fixture", "", cx);
            });
            shell.panes[0].as_mut().unwrap().selected.push(2);
        })
    });
    settle(cx);
    (shell, cx)
}

pub(super) fn row(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    kind: Kind,
) -> (usize, gpui::Point<gpui::Pixels>) {
    shell.read_with(cx, |shell, cx| {
        let producer = shell.rail.as_ref().unwrap().places.read(cx);
        let index = producer
            .entries()
            .iter()
            .position(|entry| entry.kind == kind)
            .unwrap();
        let laid = producer.laid.borrow();
        let rect = laid.row(index).expect("painted rail row").rect;
        (
            index,
            point(px(rect.x + 50.), px(rect.y + rect.height / 2.)),
        )
    })
}

fn click(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>, button: MouseButton) {
    cx.simulate_event(MouseDownEvent {
        position: at,
        button,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position: at,
        button,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    settle(cx);
}

fn unchanged(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    shell.read_with(cx, |shell, _| {
        let pane = shell.panes[0].as_ref().unwrap();
        assert_eq!(pane.path, "/files");
        assert_eq!(pane.selected.to_vec(), vec![2]);
    });
}

#[gpui::test]
fn pb_rail_2_ey165_right_click_reads_favourite_menu_without_navigation(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, true);
    let (_, at) = row(&shell, cx, Kind::Favourite);
    click(cx, at, MouseButton::Right);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    assert_eq!(
        read(&shell, cx, "contextMenuEntries", &[]),
        "Open|Open in New Tab|-|Open in Terminal|Copy Path|Remove from Favorites"
    );
    shell.read_with(cx, |shell, cx| {
        assert!(
            shell.rail.as_ref().unwrap().host.read(cx).rail.wants().menu,
            "the rail stays up under its own menu"
        )
    });
    unchanged(&shell, cx);
    choose(&shell, cx, "copypath");
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        "/docs"
    );
    unchanged(&shell, cx);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rail_2_ey176_m_reads_menu_and_opens_docs_in_a_new_tab(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, true);
    let (index, _) = row(&shell, cx, Kind::Favourite);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.rail_focus(true, cx);
            shell
                .rail
                .as_ref()
                .unwrap()
                .places
                .update(cx, |producer, cx| producer.set_cursor(index, cx));
            window.focus(&shell.dual_focus.rail, cx);
        })
    });
    settle(cx);
    cx.simulate_keystrokes("m");
    settle(cx);
    assert_eq!(
        read(&shell, cx, "contextMenuEntries", &[]),
        "Open|Open in New Tab|-|Open in Terminal|Copy Path|Remove from Favorites"
    );
    unchanged(&shell, cx);
    choose(&shell, cx, "openTab");
    settle(cx);
    assert_eq!(read(&shell, cx, "tabCount", &[]), "2");
    assert!(super::test_shell::sent()
        .iter()
        .any(|line| line.contains("/docs")));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rail_2_ey176_right_click_trash_reads_restore_all(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, true);
    let (_, at) = row(&shell, cx, Kind::Trash);
    click(cx, at, MouseButton::Right);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    assert_eq!(
        read(&shell, cx, "contextMenuEntries", &[]),
        "Open|-|Restore All|-|Empty Trash…"
    );
    unchanged(&shell, cx);
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rail_2_quiet_default_keeps_the_remove_only_favourite_menu(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, false);
    let (_, at) = row(&shell, cx, Kind::Favourite);
    click(cx, at, MouseButton::Right);
    assert_eq!(
        read(&shell, cx, "contextMenuEntries", &[]),
        "Remove from Favorites"
    );
    unchanged(&shell, cx);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// BLOCKED-LEASE g2/rail_places (I50-16 owns g2/rail_places/**): the favourite inspection needs producer.rs and service.rs. Draft: .superpowers/PB-RAIL-2/producer-inspect.rs.pending.
#[ignore = "BLOCKED-LEASE g2/rail_places: favourite inspection, see RESULT.md"]
#[gpui::test]
fn pb_rail_2_visible_favourite_is_inspected_and_carries_the_diagnostic(cx: &mut TestAppContext) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/PB-RAIL-2");
    std::fs::create_dir_all(&root).unwrap();
    let out = std::process::Command::new("mktemp")
        .args(["-d", root.join("inspect.XXXXXX").to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());
    let dir = std::path::PathBuf::from(String::from_utf8(out.stdout).unwrap().trim());
    std::fs::write(dir.join(".flea-test-sandbox"), "PB-RAIL-2").unwrap();
    let exe = dir.join("flea");
    std::fs::write(&exe, r###"#!/bin/sh
printf '%s\n' '{"statuses":[{"index":0,"record":{"label":"Missing","path":"/gone"},"error":"Not mounted"}]}'
"###).unwrap();
    flea::executable_file::write(&exe, &std::fs::read(&exe).unwrap(), 0o700).unwrap();
    let (shell, cx) = open(cx, true);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let mut state = shell.launch.settings.data.clone();
            crate::g2::settings_pages::seams::merge(
                &mut state,
                &jsondoc::parse(
                    r#"{"places":{"favourites":[{"label":"Missing","path":"/gone"}]}}"#,
                )
                .unwrap(),
            );
            shell.apply_settings(state, window, cx);
            let producer = shell.rail.as_ref().unwrap().places.clone();
            let places = producer.read(cx).places.clone();
            places.update(cx, |places, _| places.exe = exe.clone());
        })
    });
    settle(cx);
    shell.read_with(cx, |shell, cx| {
        let producer = shell.rail.as_ref().unwrap().places.read(cx);
        assert_eq!(
            producer
                .places
                .read(cx)
                .favourites
                .statuses
                .get(&0)
                .map(String::as_str),
            Some("Not mounted")
        );
        assert_eq!(
            producer
                .provider
                .drawn
                .iter()
                .flat_map(|section| &section.rows)
                .find(|row| row.label == "Missing")
                .unwrap()
                .error
                .as_deref(),
            Some("Not mounted")
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rail_2_places_menu_adds_the_row_folder_to_favourites(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, true);
    let (_, at) = row(&shell, cx, Kind::Home);
    click(cx, at, MouseButton::Right);
    choose(&shell, cx, "addFavourite");
    settle(cx);
    shell.read_with(cx, |shell, _| {
        let records = shell
            .launch
            .settings
            .data
            .get("places")
            .unwrap()
            .get("favourites")
            .unwrap()
            .as_array()
            .unwrap();
        assert!(records.iter().any(|record| record
            .get("path")
            .and_then(flea::jsondoc::Json::as_str)
            == Some("/fixture")));
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rail_2_changed_favourite_refuses_remove_from_an_old_menu(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, true);
    let (_, at) = row(&shell, cx, Kind::Favourite);
    click(cx, at, MouseButton::Right);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let mut state = shell.launch.settings.data.clone();
            crate::g2::settings_pages::seams::merge(
                &mut state,
                &jsondoc::parse(r#"{"places":{"favourites":[{"label":"Other","path":"/other"}]}}"#)
                    .unwrap(),
            );
            shell.apply_settings(state, window, cx);
        })
    });
    settle(cx);
    choose(&shell, cx, "removeFavourite");
    assert_eq!(
        super::test_shell::status(&shell, cx),
        "Favorites changed; reopen the menu before removing this row."
    );
    shell.read_with(cx, |shell, cx| {
        let producer = shell.rail.as_ref().unwrap().places.read(cx);
        assert!(!producer.places.read(cx).favourites.busy);
        assert_eq!(
            producer.places.read(cx).favourites.records[0]
                .get("path")
                .and_then(flea::jsondoc::Json::as_str),
            Some("/other")
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// EY-358: with the rail on auto-hide, Escape on a favourite's menu returns the keyboard to that row and the rail stays up.
#[gpui::test]
fn ey358_escape_on_an_auto_hidden_favourite_menu_returns_to_the_rail_row(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, true);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let mut state = shell.launch.settings.data.clone();
            crate::g2::settings_pages::seams::merge(
                &mut state,
                &jsondoc::parse(r#"{"places":{"autoHide":true}}"#).unwrap(),
            );
            shell.apply_settings(state, window, cx);
        })
    });
    settle(cx);
    let index = shell.read_with(cx, |shell, cx| {
        let producer = shell.rail.as_ref().unwrap().places.read(cx);
        assert_eq!(
            shell.rail.as_ref().unwrap().host.read(cx).rail.mode(),
            crate::rail::Mode::Overlay
        );
        producer
            .entries()
            .iter()
            .position(|entry| entry.kind == Kind::Favourite)
            .unwrap()
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.rail_focus(true, cx);
            shell
                .rail
                .as_ref()
                .unwrap()
                .places
                .update(cx, |producer, cx| producer.set_cursor(index, cx));
            window.focus(&shell.dual_focus.rail, cx);
        })
    });
    settle(cx);
    cx.simulate_keystrokes("m");
    settle(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert!(
            shell.dual_focus.rail.contains_focused(window, cx),
            "the keyboard is back on the rail"
        );
        assert_eq!(shell.keyboard.context, flea::keys::Context::Rail);
        let join = shell.rail.as_ref().unwrap();
        assert!(
            join.host.read(cx).rail.revealed(),
            "the rail stays revealed"
        );
        assert_eq!(
            join.places.read(cx).cursor(),
            Some(index),
            "the cursor is on the row the menu opened from"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[path = "rail_overlay_focus_tests.rs"]
mod overlay_focus;

#[path = "rail_reveal_hover_tests.rs"]
mod reveal_hover;
