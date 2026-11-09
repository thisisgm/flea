//! Menu rows reach their handler through Shell::dispatch on the production Shell, by keys and a right click.
use super::test_shell::{land, open_root as open, status};
use crate::{
    backend::Event as BackendEvent, g2::mn1::join::Chosen, state::Shell, window::u07_tests::read,
};
use flea::{
    jsondoc::{self, Json},
    protocol::RowFacts,
};
use gpui::{
    point, px, Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, VisualTestContext,
};
use std::{cell::RefCell, rc::Rc, time::Duration};

const DIR: u32 = 0o040755;
const FILE: u32 = 0o100644;

pub(super) fn entry(name: &str, mode: u32) -> RowFacts {
    RowFacts {
        name: name.into(),
        dir: mode & 0o170000 == 0o040000,
        mode,
        icon: if mode == DIR { "folder" } else { "text-plain" }.into(),
        ..RowFacts::default()
    }
}

pub(super) fn home() -> Vec<RowFacts> {
    vec![
        entry("alpha", DIR),
        entry("beta", DIR),
        entry("notes.txt", FILE),
        entry("todo.md", FILE),
    ]
}

pub(super) fn reply(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    deliver(shell, cx, line);
    if !shell.read_with(cx, |shell, _| shell.menu.tags_reading) {
        settle(cx);
    }
}

// One backend line with no clock advance, as a reply that lands before the menu's loading mark is due; an open menu's own pane's backend sends it.
pub(super) fn deliver(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let side = shell.menu.captured.as_ref().map_or(0, |c| c.side);
            shell.receive(side, BackendEvent::Producer(line.into()), window, cx)
        })
    });
}

pub(super) fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
}

fn captured(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> u64 {
    shell.read_with(cx, |shell, _| {
        shell.menu.captured.as_ref().expect("an open menu").id
    })
}

/// Answers the row menu's snapshot with these type bits and its application producer with none.
pub(super) fn answer(shell: &Entity<Shell>, cx: &mut VisualTestContext, kinds: &[u32]) {
    let id = captured(shell, cx);
    let kinds = kinds
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    deliver(shell, cx, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[{kinds}]}}"));
    reply(shell, cx, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"applications\",\"ok\":true,\"applications\":[]}}"));
}

pub(super) fn tags_answer(shell: &Entity<Shell>, cx: &mut VisualTestContext, kinds: &[u32]) {
    answer(shell, cx, kinds);
    let answer = shell.read_with(cx, |shell, _| {
        let (id, listing) = shell.menu.tags.asked_for_test()?;
        let capture = shell.menu.captured.as_ref()?;
        let host = shell.panes[capture.side].as_ref()?;
        let files = capture
            .rows
            .iter()
            .map(|row| {
                host.pane
                    .held
                    .row(*row)
                    .map(|facts| facts.tags.clone().unwrap_or_default())
            })
            .collect::<Option<Vec<_>>>()?;
        use flea::ui_logic_pl4::tags::{self, State};
        let states = tags::states_for(&files);
        let names = |state| {
            tags::NAMES
                .iter()
                .zip(states)
                .filter(|(_, value)| *value == state)
                .map(|(name, _)| format!("\"{name}\""))
                .collect::<Vec<_>>()
                .join(",")
        };
        Some(format!(
            r#"{{"t":"tagstates","id":{id},"listing":{listing},"applied":[{}],"mixed":[{}]}}"#,
            names(State::Applied),
            names(State::Mixed)
        ))
    });
    if let Some(answer) = answer {
        reply(shell, cx, &answer);
    }
}

pub(super) fn actions(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<String> {
    let model = jsondoc::parse(&read(shell, cx, "contextMenuModel", &[])).unwrap();
    // One entry per drawn row, a separator as an empty action, so an index is the menu cursor's.
    model
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            row.get("action")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_owned()
        })
        .collect()
}

/// Walks the drawn menu with Down and Up to the row, then presses Return on it.
pub(super) fn choose(shell: &Entity<Shell>, cx: &mut VisualTestContext, action: &str) {
    let index = actions(shell, cx)
        .iter()
        .position(|row| row == action)
        .unwrap_or_else(|| panic!("the menu offers {action}: {:?}", actions(shell, cx)));
    for _ in 0..64 {
        let cursor: usize = read(shell, cx, "contextMenuCursor", &[]).parse().unwrap();
        if cursor == index {
            cx.simulate_keystrokes("enter");
            settle(cx);
            return;
        }
        cx.simulate_keystrokes(if cursor < index { "down" } else { "up" });
    }
    panic!("the keyboard must reach {action}");
}

/// The backend's validate answer for the walked row, naming the paths its snapshot holds.
pub(super) fn validated(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    action: &str,
    paths: &[&str],
) {
    let id = captured(shell, cx);
    let paths = paths
        .iter()
        .map(|p| format!("\"{p}\""))
        .collect::<Vec<_>>()
        .join(",");
    reply(shell, cx, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"validate\",\"ok\":true,\"action\":\"{action}\",\"paths\":[{paths}]}}"));
}

pub(super) fn right_click(cx: &mut VisualTestContext, x: f32, y: f32) {
    let position = point(px(x), px(y));
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Right,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position,
        button: MouseButton::Right,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    settle(cx);
}

fn favourites(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<String> {
    shell.read_with(cx, |shell, _| {
        let data = &shell.launch.settings.data;
        data.get("places")
            .and_then(|places| places.get("favourites"))
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|row| row.get("path").and_then(Json::as_str))
            .map(str::to_owned)
            .collect()
    })
}

fn selected(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<usize> {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().selected.clone()
    })
}

const UNAVAILABLE: &str = "That action is no longer available; reopen the menu.";

#[gpui::test]
fn a_walked_folder_row_adds_its_favourite_and_never_answers_unavailable(
    cx: &mut gpui::TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[DIR]);
    choose(&shell, cx, "addFavourite");
    validated(&shell, cx, "addFavourite", &["/files/alpha"]);
    assert_eq!(
        favourites(&shell, cx),
        ["/files/alpha"],
        "the walked row ran"
    );
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    assert!(!shell.read_with(cx, |shell, _| shell.menu_opened()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn background_rows_select_invert_toggle_code_icons_add_the_folder_and_open_settings(
    cx: &mut gpui::TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    // Below the four rows the right click lands on the background.
    let below = (600., 500.);
    right_click(cx, below.0, below.1);
    assert!(
        !actions(&shell, cx).contains(&"invertSelection".to_owned()),
        "Invert Selection ships hidden"
    );
    choose(&shell, cx, "selectAll");
    assert_eq!(
        selected(&shell, cx),
        [0, 1, 2, 3],
        "Select All marks every drawn row"
    );
    right_click(cx, below.0, below.1);
    let shown = actions(&shell, cx);
    assert!(
        shown.iter().any(|row| row == "toggleLangMarks"),
        "the background menu draws Hide Code Icons"
    );
    choose(&shell, cx, "toggleLangMarks");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .launch
            .settings
            .data
            .get("langMarks")
            .and_then(Json::as_bool)),
        Some(false),
        "Hide Code Icons writes langMarks off"
    );
    // The settings change reads the folder again, as every ui.json write that a view draws from does.
    land(&shell, cx, "/files", home(), 2);
    right_click(cx, below.0, below.1);
    choose(&shell, cx, "addFavourite");
    assert_eq!(
        favourites(&shell, cx),
        ["/files"],
        "the background adds its own folder"
    );
    right_click(cx, below.0, below.1);
    choose(&shell, cx, "settings");
    assert!(
        shell.read_with(cx, |shell, _| shell.settings.opened),
        "Settings… opens Settings"
    );
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn invert_selection_flips_the_marks_when_settings_shows_it(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = open(cx, "default");
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.launch.settings.data =
                jsondoc::parse(r#"{"keys":"default","view":"list","menu":{"hidden":[]}}"#).unwrap();
        })
    });
    land(&shell, cx, "/files", home(), 1);
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.panes[0].as_mut().unwrap().selected = vec![1]
        })
    });
    right_click(cx, 600., 500.);
    choose(&shell, cx, "invertSelection");
    assert_eq!(selected(&shell, cx), [0, 2, 3]);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[path = "menus_formats_tests.rs"]
mod extracted;

#[path = "menus_refuse_tests.rs"]
mod refuse;

#[gpui::test]
fn a_search_results_go_to_location_row_takes_the_file_to_its_folder(
    cx: &mut gpui::TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    let mut track = entry("Music/Album/track-60.flac", FILE);
    track.icon = "audio-x-generic".into();
    land(&shell, cx, "/files", vec![entry("Music", DIR), track], 1);
    // A walk owns the rows, as fixmenus' search fixture sets it: the results name files under the base.
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.search.owner.session.base = "/files".into();
        host.search.owner.session.query = "track".into();
        host.search.owner.session.mode = flea::ui_logic_sr1::Mode::Results;
    });
    settle(cx);
    cx.simulate_keystrokes("j");
    super::test_shell::sent();
    // The pointer's own entrance, as a right click on a result takes: open_menu moves the cursor there.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.open_menu(0, Some(1), (300., 80.), window, cx))
    });
    cx.run_until_parked();
    answer(&shell, cx, &[FILE]);
    let shown = actions(&shell, cx);
    assert!(
        shown.contains(&"goToLocation".to_owned()),
        "the search results offer their own way to the file: {shown:?}"
    );
    choose(&shell, cx, "goToLocation");
    validated(&shell, cx, "goToLocation", &["/files/Music/Album/track-60.flac"]);
    assert_eq!(
        super::test_shell::asked(&shell, cx),
        "/files/Music/Album",
        "the reveal lists the file's own folder"
    );
    let mut back = entry("track-60.flac", FILE);
    back.icon = "audio-x-generic".into();
    land(&shell, cx, "/files/Music/Album", vec![entry("sleeve.png", FILE), back], 30);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 1, "the revealed file lands selected");
        assert_eq!(host.pane.held.row(1).unwrap().name, "track-60.flac");
        assert!(!host.search.owner.active(), "the walk is closed");
        assert!(!shell.menu_opened(), "the menu retired");
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
