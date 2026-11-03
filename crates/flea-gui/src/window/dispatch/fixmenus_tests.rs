//! FIX-MENUS through the mounted window: the zip probe (EY-488), the filtered ground (EY-504) and m on a rail row (EY-541).
use super::{
    menus_dual_tests::spot,
    menus_tests::{actions, answer, deliver, entry, reply, right_click, settle},
    rail_menu_tests,
    test_shell::{land, open_root, sent},
};
use crate::{state::Shell, window::u07_tests::read};
use flea::{protocol::RowFacts, ui_logic_pl4::rail::Kind};
use gpui::{px, Entity, TestAppContext, VisualTestContext};

const FILE: u32 = 0o100644;
const ISO_ANSWER: &str = r#"{"t":"flashimage","path":"/files/archive-iso.zip","kind":"zip","bytes":1024,"unpacked":4096,"known":true,"member":"archive-iso.iso"}"#;

// The flashimage asks among the lines the window sent since the last read.
fn zip_asks() -> Vec<String> {
    sent()
        .into_iter()
        .filter(|line| line.contains(r#""c":"flashimage""#))
        .collect()
}

fn ask(name: &str) -> [String; 1] {
    [format!(r#"{{"c":"flashimage","path":"/files/{name}"}}"#)]
}

// Opens the row menu on the cursor with the m key and reads whether Flash to drive is among its rows.
fn offers_flash(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    cx.simulate_keystrokes("m");
    answer(shell, cx, &[FILE]);
    let offered = actions(shell, cx).iter().any(|row| row == "flashDrive");
    cx.simulate_keystrokes("escape");
    settle(cx);
    offered
}

// A pointer menu on a row the cursor is not on: begin_menu moves the cursor there, as a right click does.
fn open_on_row(shell: &Entity<Shell>, cx: &mut VisualTestContext, row: usize) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_menu(0, Some(row), (300., 80.), window, cx)
        })
    });
    // No clock advance, as offers_flash's m key: the snapshot answer lands before the loading mark is due.
    cx.run_until_parked();
}

fn sized(name: &str, size: u64) -> RowFacts {
    RowFacts {
        size,
        ..entry(name, FILE)
    }
}

#[gpui::test]
fn fixmenus_ey488_a_zip_whose_one_member_is_an_iso_offers_flash_to_drive(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    sent();
    let rows = |size| {
        vec![
            sized("archive-iso.zip", size),
            entry("notes.txt", FILE),
            entry("other.zip", FILE),
        ]
    };
    land(&shell, cx, "/files", rows(1024), 1);
    assert_eq!(
        zip_asks(),
        ask("archive-iso.zip"),
        "the cursor zip is asked about once"
    );
    settle(cx);
    assert!(zip_asks().is_empty(), "a redraw never asks again");
    deliver(&shell, cx, ISO_ANSWER);
    assert!(offers_flash(&shell, cx));
    cx.simulate_keystrokes("down");
    settle(cx);
    assert!(zip_asks().is_empty(), "a plain file asks nothing");
    cx.simulate_keystrokes("down");
    settle(cx);
    assert_eq!(zip_asks(), ask("other.zip"));
    // The first zip's answer arrives late, after the target moved on.
    deliver(&shell, cx, ISO_ANSWER);
    assert!(
        !offers_flash(&shell, cx),
        "a new target forgets the last member, and a late answer for another path never lands"
    );
    cx.simulate_keystrokes("up");
    cx.simulate_keystrokes("up");
    settle(cx);
    assert_eq!(zip_asks(), ask("archive-iso.zip"));
    land(&shell, cx, "/files", rows(1024), 2);
    assert!(
        zip_asks().is_empty(),
        "the same zip relisted unchanged asks nothing"
    );
    land(&shell, cx, "/files", rows(2048), 3);
    assert_eq!(
        zip_asks(),
        ask("archive-iso.zip"),
        "a zip rewritten in place is asked about again"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn fixmenus_ey488_a_pointer_menu_on_a_zip_asks_before_its_snapshot(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![entry("notes.txt", FILE), entry("archive-iso.zip", FILE)],
        1,
    );
    settle(cx);
    sent();
    open_on_row(&shell, cx, 1);
    let lines = sent();
    let at = |word: &str| lines.iter().position(|line| line.contains(word));
    assert!(
        matches!(
            (at(r#""c":"flashimage""#), at(r#""op":"snapshot""#)),
            (Some(ask), Some(snapshot)) if ask < snapshot
        ),
        "{lines:?}"
    );
    deliver(&shell, cx, ISO_ANSWER);
    answer(&shell, cx, &[FILE]);
    let shown = actions(&shell, cx);
    assert!(
        shown.iter().any(|row| row == "flashDrive"),
        "an answer that lands before the menu draws reaches it: {shown:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn fixmenus_ey504_under_a_filter_the_ground_below_the_drawn_rows_opens_the_folder_menu(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_root(cx, "default");
    let mut rows: Vec<_> = (0..10)
        .map(|at| entry(&format!("file-{at}.txt"), FILE))
        .collect();
    rows[3] = entry("notes.txt", FILE);
    land(&shell, cx, "/files", rows, 1);
    cx.simulate_keystrokes("/ n");
    reply(
        &shell,
        cx,
        r#"{"t":"filtered","id":1,"query":"n","n":10,"rows":[3],"ms":0.01,"listing":1}"#,
    );
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().filter.shown.as_deref(),
            Some(&[3][..])
        );
    });
    cx.simulate_keystrokes("down");
    settle(cx);
    let (x, y) = spot(&shell, cx, 0, Some(3));
    // One row lower is still inside the frame's held span of ten rows, but nothing is drawn there.
    let below = y + shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap().row
    });
    shell.read_with(cx, |shell, _| {
        let input = &shell.panes[0].as_ref().unwrap().input;
        assert_eq!(input.hit(gpui::point(px(x), px(y))), Some(3));
        assert_eq!(
            input.hit(gpui::point(px(x), px(below))),
            None,
            "a click below the drawn rows names no hidden row"
        );
    });
    right_click(cx, x, below);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .menu
            .captured
            .as_ref()
            .map(|capture| capture.background)),
        Some(true),
        "v0.3.12 opens the folder menu on the ground below the filtered rows"
    );
    assert!(actions(&shell, cx).iter().any(|row| row == "selectAll"));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// EY-541: Tab lights the rail's Home row; m says why no menu opens under the default Menus switches, as Mounts.raiseMenu does, and opens the place menu once it is on (ruling 5).
#[gpui::test]
fn fixmenus_ey541_m_on_the_tabbed_rail_home_row_answers_or_opens_its_place_menu(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = rail_menu_tests::open(cx, false);
    let (home, at) = rail_menu_tests::row(&shell, cx, Kind::Home);
    // A right click on a row without a menu stays quiet, and leaves the rail's pointer focus set for the m below.
    right_click(cx, at.x.into(), at.y.into());
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    assert_eq!(read(&shell, cx, "lastMessage", &[]), "");
    cx.simulate_keystrokes("tab");
    // The Boat capture draws the keyboard cursor on Home after Tab; the fixture rail seeds no cursor, so it is placed there.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(
                shell.dual_focus.rail.contains_focused(window, cx),
                "Tab focuses the rail"
            );
            let places = shell.rail.as_ref().unwrap().places.clone();
            places.update(cx, |producer, cx| producer.set_cursor(home, cx));
        })
    });
    settle(cx);
    cx.simulate_keystrokes("m");
    settle(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "Home has nothing to eject or unmount"
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let mut state = shell.launch.settings.data.clone();
            crate::g2::st1::seams::merge(
                &mut state,
                &flea::jsondoc::parse(r#"{"menu":{"hidden":[]}}"#).unwrap(),
            );
            shell.apply_settings(state, window, cx);
        })
    });
    settle(cx);
    right_click(cx, at.x.into(), at.y.into());
    let raised = read(&shell, cx, "contextMenuEntries", &[]);
    assert!(raised.starts_with("Open|"), "{raised}");
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.update(|window, cx| {
        let shell = shell.read(cx);
        assert!(shell.dual_focus.rail.contains_focused(window, cx));
    });
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    cx.simulate_keystrokes("m");
    settle(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    assert_eq!(read(&shell, cx, "contextMenuEntries", &[]), raised);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// A search row names its file relative to the search base, so the waiting menu takes the answer by its whole path.
#[gpui::test]
fn fixmenus_ey488_a_search_row_zip_answer_reaches_its_waiting_menu(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![entry("notes.txt", FILE), entry("sub/archive-iso.zip", FILE)],
        1,
    );
    // The pane's path is elsewhere, as makeexec's search test sets it, so only the search base names the row's folder.
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.path = "/elsewhere".into();
        host.search.owner.session.base = "/files".into();
    });
    settle(cx);
    sent();
    open_on_row(&shell, cx, 1);
    assert_eq!(
        zip_asks(),
        ask("sub/archive-iso.zip"),
        "the ask names the row under the search base, never the pane's path"
    );
    // An answer for another zip reaches the waiting menu first and must not arm it.
    deliver(&shell, cx, ISO_ANSWER);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.menu.context.flash_zip_member.clone()),
        ""
    );
    deliver(
        &shell,
        cx,
        &ISO_ANSWER.replace("/files/archive-iso.zip", "/files/sub/archive-iso.zip"),
    );
    answer(&shell, cx, &[FILE]);
    let shown = actions(&shell, cx);
    assert!(
        shown.iter().any(|row| row == "flashDrive"),
        "the answer for sub/archive-iso.zip reaches the menu on that row: {shown:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// EY-541 as reported: Tab to the rail, then m, with no pointer on the rail before it.
#[gpui::test]
fn fixmenus_ey541_tab_then_m_on_rail_home_says_why_no_menu_opens(cx: &mut TestAppContext) {
    let (shell, cx) = rail_menu_tests::open(cx, false);
    let (home, _) = rail_menu_tests::row(&shell, cx, Kind::Home);
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.dual_focus.rail.contains_focused(window, cx));
            let places = shell.rail.as_ref().unwrap().places.clone();
            places.update(cx, |producer, cx| producer.set_cursor(home, cx));
        })
    });
    settle(cx);
    cx.simulate_keystrokes("m");
    settle(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "Home has nothing to eject or unmount"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn fixmenus_ey488_an_answer_after_the_menu_draws_leaves_its_rows(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![entry("notes.txt", FILE), entry("archive-iso.zip", FILE)],
        1,
    );
    settle(cx);
    open_on_row(&shell, cx, 1);
    answer(&shell, cx, &[FILE]);
    deliver(&shell, cx, ISO_ANSWER);
    let shown = actions(&shell, cx);
    assert!(
        shown.iter().any(|row| row == "open") && !shown.iter().any(|row| row == "flashDrive"),
        "a drawn menu keeps its rows, as noteZip never rebuilds one: {shown:?}"
    );
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(
        offers_flash(&shell, cx),
        "the late answer was taken, so the next open offers it"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
