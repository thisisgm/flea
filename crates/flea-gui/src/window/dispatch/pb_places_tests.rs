//! Favorites menus are reached by right clicking the rail in a mounted Root window.
use super::{
    menus_tests::{actions, choose, right_click, settle},
    test_shell::{asked, land, open_root_with, row},
};
use crate::{state::Shell, window::u07_tests::read};
use flea::ui_logic_pl4::{
    order::{Heading, RailSection},
    rail::{Group, Kind, RailEntry},
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::cell::RefCell;

thread_local! {
    static LAUNCHED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}

fn open_menu(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    menu_fixture(cx, false)
}

pub(crate) fn menu_fixture(
    cx: &mut TestAppContext,
    shipped: bool,
) -> (Entity<Shell>, &mut VisualTestContext) {
    let settings = if shipped {
        r#""view":"list","keys":"default""#
    } else {
        r#""view":"list","keys":"default","menu":{"hidden":[]}"#
    };
    let (shell, cx) = open_root_with(cx, settings, None);
    let record = flea::jsondoc::parse(r#"{"path":"/target","label":"Work"}"#).unwrap();
    shell.update(cx, |shell, _| {
        let patch = flea::jsondoc::parse(
            r#"{"places":{"favourites":[{"path":"/target","label":"Work"}]}}"#,
        )
        .unwrap();
        shell.launch.settings.data =
            flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
    });
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    let producer = shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().places.clone());
    producer.update(cx, |producer, cx| {
        producer.replace_sections(
            vec![RailSection {
                heading: Heading::Key("places.heading.favourites"),
                rows: vec![RailEntry {
                    group: Group::Favourite,
                    kind: Kind::Favourite,
                    label: "Work".into(),
                    path: "/target".into(),
                    glyph: "folder",
                    favourite_index: Some(0),
                    original: Some(record),
                    ..RailEntry::default()
                }],
            }],
            cx,
        )
    });
    shell.update(cx, |shell, _| {
        LAUNCHED.with(|calls| calls.borrow_mut().clear());
        shell.opener[0].launch = |argv| {
            LAUNCHED.with(|calls| calls.borrow_mut().push(argv.to_vec()));
            (Some(0), String::new())
        };
    });
    settle(cx);
    let at = read(&shell, cx, "railRowCentre", &["0"]);
    let xy: Vec<f32> = at.split_whitespace().map(|v| v.parse().unwrap()).collect();
    assert_eq!(xy.len(), 2, "the rail row must be drawn: {at:?}");
    right_click(cx, xy[0], xy[1]);
    let state = flea::jsondoc::parse(&read(&shell, cx, "menuState", &[])).unwrap();
    assert_eq!(
        state.get("opened").and_then(flea::jsondoc::Json::as_bool),
        Some(true),
        "right click must open the Favorites menu"
    );
    (shell, cx)
}

#[gpui::test]
fn pb_places_remote_favorite_enter_mounts_its_share(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list","keys":"default""#, None);
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    let producer = shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().places.clone());
    producer.update(cx, |producer, cx| {
        producer.replace_sections(
            vec![RailSection {
                heading: Heading::Key("places.heading.favourites"),
                rows: vec![RailEntry {
                    group: Group::Favourite,
                    kind: Kind::Favourite,
                    label: "NAS".into(),
                    path: "sftp://host/".into(),
                    glyph: "server",
                    favourite_index: Some(0),
                    ..RailEntry::default()
                }],
            }],
            cx,
        )
    });
    settle(cx);
    shell.update(cx, |shell, cx| shell.rail_focus(true, cx));
    // Focus leaves the rail cursor unset, and Enter opens only the row under it, so Down moves it there first.
    let labels = read(&shell, cx, "railLabels", &[]);
    let index = labels
        .split('|')
        .position(|label| label == "NAS")
        .expect("the NAS favourite is listed") as i64;
    for _ in 0..8 {
        let cursor: i64 = read(&shell, cx, "railCursor", &[]).parse().unwrap();
        if cursor == index {
            break;
        }
        cx.simulate_keystrokes(if cursor < index { "down" } else { "up" });
        settle(cx);
    }
    assert_eq!(
        read(&shell, cx, "railCursor", &[]),
        index.to_string(),
        "Down puts the rail cursor on the NAS favourite"
    );
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert_eq!(
        read(&shell, cx, "networkResult", &[]),
        "mounting",
        "a remote Favorite opens by mounting its share, as a NETWORK row does"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_places_favorite_open_navigates_its_captured_path(cx: &mut TestAppContext) {
    let (shell, cx) = open_menu(cx);
    choose(&shell, cx, "open");
    assert_eq!(asked(&shell, cx), "/target");
    let state = flea::jsondoc::parse(&read(&shell, cx, "menuState", &[])).unwrap();
    assert_eq!(
        state.get("opened").and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_places_favorite_terminal_uses_the_place_not_the_listing(cx: &mut TestAppContext) {
    let (shell, cx) = open_menu(cx);
    choose(&shell, cx, "openTerminal");
    let calls = LAUNCHED.with(|calls| calls.take());
    assert_eq!(calls.len(), 1);
    assert_eq!(&calls[0][1..], ["--terminal", "/target"]);
    assert_eq!(asked(&shell, cx), "/files");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_places_shipped_favorite_menu_keeps_only_remove(cx: &mut TestAppContext) {
    let (shell, cx) = menu_fixture(cx, true);
    assert_eq!(actions(&shell, cx), ["removeFavourite"]);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_places_changed_favorite_is_not_removed_by_a_stale_menu(cx: &mut TestAppContext) {
    let (shell, cx) = menu_fixture(cx, true);
    shell.update(cx, |shell, _| {
        let patch = flea::jsondoc::parse(
            r#"{"places":{"favourites":[{"path":"/other","label":"Other"}]}}"#,
        )
        .unwrap();
        shell.launch.settings.data =
            flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
    });
    choose(&shell, cx, "removeFavourite");
    assert_eq!(
        super::test_shell::status(&shell, cx),
        "Favorites changed; reopen the menu before removing this row."
    );
    shell.read_with(cx, |shell, _| assert!(shell.settings.pending.is_empty()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
