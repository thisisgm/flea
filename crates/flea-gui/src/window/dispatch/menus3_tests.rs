//! Menu entrances driven through the mounted native root and read back from its drawn cards.
use super::{
    menus_tests::{actions, answer, choose, entry, reply, right_click, settle},
    test_shell::{asked, keystroke, land, open_root, open_root_with, sent, status},
};
use crate::{columns::geometry::Slot, state::Shell, window::u07_tests::read};
use flea::{jsondoc, keys, protocol::RowFacts};
use gpui::{Entity, TestAppContext, VisualTestContext};

const FILE: u32 = 0o100644;
const DIR: u32 = 0o040755;

fn closed(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    assert_eq!(read(shell, cx, "contextMenuVisible", &[]), "false");
    assert!(shell.read_with(cx, |shell, _| shell.menu.captured.is_none()));
}

#[gpui::test]
fn ey104_empty_listing_ground_opens_the_background_menu_at_the_pointer(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", Vec::new(), 1);
    right_click(cx, 600., 350.);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true");
    let shown = actions(&shell, cx);
    for action in [
        "newFolder",
        "newFile",
        "selectAll",
        "openTerminal",
        "sort",
        "settings",
    ] {
        assert!(shown.iter().any(|row| row == action), "{shown:?}");
    }
    shell.read_with(cx, |shell, _| {
        assert!(shell.menu.captured.as_ref().unwrap().background);
        let menu = shell.menu.menu.borrow();
        assert_eq!(menu.card.as_ref().unwrap().frame.x, 600.);
    });
    cx.simulate_keystrokes("escape");
    right_click(cx, 600., 690.);
    closed(&shell, cx);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
#[ignore = "known red until the header menu column toggle lands (EY-124)"]
fn ey124_header_menu_toggles_columns_and_keeps_sort_and_cursor(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![entry("notes.txt", FILE)], 1);
    right_click(cx, 770., 40.);
    assert_eq!(
        actions(&shell, cx),
        [
            "col:mode",
            "col:size",
            "col:date",
            "col:kind",
            "",
            "toggleHidden"
        ]
    );
    assert_eq!(read(&shell, cx, "contextMenuCursor", &[]), "0");
    sent();
    choose(&shell, cx, "col:size");
    closed(&shell, cx);
    let settings = read(&shell, cx, "uiSettings", &[]);
    let settings = jsondoc::parse(&settings).unwrap();
    let columns = settings.get("columns").unwrap().as_array().unwrap();
    assert_eq!(
        columns,
        &[
            jsondoc::Json::Str("name".into()),
            jsondoc::Json::Str("date".into())
        ]
    );
    assert_eq!(read(&shell, cx, "cursor", &[]), "0");
    assert!(!sent().iter().any(|line| line.contains(r#""c":"sort""#)));
    right_click(cx, 770., 40.);
    let labels = read(&shell, cx, "contextMenuEntries", &[]);
    assert!(labels.contains("Show Size"), "{labels}");
    choose(&shell, cx, "toggleHidden");
    closed(&shell, cx);
    assert!(shell.read_with(cx, |shell, _| shell.panes[0]
        .as_ref()
        .unwrap()
        .nav
        .folders
        .borrow()
        .hidden));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey129_menu_key_in_an_empty_folder_refuses_without_opening_a_menu(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", Vec::new(), 1);
    cx.simulate_keystrokes("m");
    settle(cx);
    closed(&shell, cx);
    assert_eq!(
        status(&shell, cx),
        "No item under the cursor to open a menu on"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey136_shift_t_opens_the_tags_flyout_on_the_cursor(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![entry("notes.txt", FILE)], 1);
    let binding = keys::binding_rows("default", keys::Frontend::Gui)
        .find(|binding| binding.action == "tags" && binding.key == "T")
        .unwrap();
    cx.simulate_event(keystroke(binding));
    assert!(
        shell.read_with(cx, |shell, _| shell.menu.captured.is_some()),
        "Shift+T opens the cursor menu"
    );
    let id = shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().unwrap().id);
    answer(&shell, cx, &[FILE]);
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"tagstates","id":{id},"listing":1,"applied":[],"mixed":[]}}"#),
    );
    assert_eq!(
        read(&shell, cx, "contextMenuVisible", &[]),
        "true",
        "{}",
        debug(&shell, cx)
    );
    let state = read(&shell, cx, "menuState", &[]);
    let state = jsondoc::parse(&state).unwrap();
    assert_eq!(
        state.get("submenu").and_then(jsondoc::Json::as_bool),
        Some(true)
    );
    shell.read_with(cx, |shell, _| {
        let menu = shell.menu.menu.borrow();
        let flyout = menu.flyout.as_ref().unwrap();
        assert_eq!(flyout.action, "tags");
        assert_eq!(flyout.card.entries.len(), 7);
        assert_eq!(flyout.card.cursor, Some(0));
        let parent = flyout.parent.unwrap();
        assert_eq!(
            menu.card.as_ref().unwrap().entries[parent].action.as_ref(),
            "tags"
        );
    });
    cx.simulate_keystrokes("escape escape");
    closed(&shell, cx);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn columns_menu(cx: &mut TestAppContext, slot: Slot, missing: bool) {
    let (shell, cx) = open_root_with(
        cx,
        r#""keys":"default","view":"columns","columnCount":3"#,
        None,
    );
    land(
        &shell,
        cx,
        "/files",
        vec![entry("A", DIR), entry("Code", DIR)],
        1,
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell
                .step(
                    0,
                    crate::window::navigate::Step::OpenPlace("/files/A".into()),
                    window,
                    cx,
                )
                .unwrap();
        })
    });
    land(
        &shell,
        cx,
        "/files/A",
        vec![entry("full", DIR), entry("alpha.txt", FILE)],
        2,
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            crate::window::columns_tests::host::peek(
                shell,
                "/files",
                &[("A", true), ("Code", true)],
                false,
                window,
                cx,
            );
            crate::window::columns_tests::host::peek(
                shell,
                "/files/A/full",
                &[("f1.txt", false)],
                false,
                window,
                cx,
            );
            crate::window::columns_tests::host::settle(shell, window, cx);
        })
    });
    settle(cx);
    let (point, path, rows, cursor): (_, &str, Vec<RowFacts>, _) = match slot {
        Slot::Parent => (
            (1, "/files"),
            "/files",
            vec![entry("A", DIR), entry("Code", DIR)],
            1,
        ),
        Slot::Child => (
            (0, "/files/A/full"),
            "/files/A/full",
            vec![entry("f1.txt", FILE)],
            0,
        ),
        _ => unreachable!(),
    };
    let (x, y) = shell.read_with(cx, |shell, _| {
        assert_eq!(shell.shown_view(), crate::window::startup::View::Columns);
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let paint = columns.painters.get(slot).unwrap().borrow();
        let drawn = paint.drawn.as_ref().expect("the neighbour was drawn");
        assert_eq!(drawn.view().path, path);
        let crate::columns::TrackRows::Peek(peek) = &drawn.view().rows else {
            panic!("a drawn peek");
        };
        assert_eq!(
            peek[point.0].name,
            if slot == Slot::Parent {
                "Code"
            } else {
                "f1.txt"
            }
        );
        let bounds = drawn.bounds;
        (
            f32::from(bounds.center().x),
            f32::from(bounds.top()) + point.0 as f32 * 31. + 15.,
        )
    });
    right_click(cx, x, y);
    // The loading gate starts its minimum stand only after the drawn frame is acknowledged.
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window.simulate_next_frame(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        asked(&shell, cx),
        "/files/A",
        "right click keeps the active listing"
    );
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "false");
    let id = shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().unwrap().id);
    if missing {
        shell.update_in(cx, |shell, window, cx| shell.receive(0, crate::backend::Event::Producer(format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":false,"error":"Could not inspect selected item."}}"#)), window, cx));
        closed(&shell, cx);
        shell.update_in(cx, |shell, window, cx| {
            shell.receive(
                0,
                crate::backend::Event::Producer(format!(
                    r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"kinds":[16384]}}"#
                )),
                window,
                cx,
            )
        });
        closed(&shell, cx);
    } else {
        answer(&shell, cx, &[rows[cursor].mode]);
        assert_eq!(
            read(&shell, cx, "contextMenuVisible", &[]),
            "true",
            "{}",
            debug(&shell, cx)
        );
        shell.read_with(cx, |shell, _| {
            let capture = shell.menu.captured.as_ref().unwrap();
            assert_eq!(capture.path, path);
            assert_eq!(capture.rows, [0]);
            assert_eq!(capture.target.as_ref().unwrap().name, rows[cursor].name);
            assert!(!capture.background);
        });
    }
    assert_eq!(asked(&shell, cx), "/files/A");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn debug(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    let state = read(shell, cx, "menuState", &[]);
    shell.read_with(cx, |shell, _| {
        format!(
            "state={state}, capture={:?}, reading={}, tags={}, at={:?}, apps={}, status={}",
            shell.menu.captured,
            shell.menu.reading,
            shell.menu.tags_reading,
            shell.menu.presentation.at,
            shell.menu.context.open_with_loaded,
            shell.panes[0].as_ref().unwrap().status
        )
    })
}

#[gpui::test]
fn ey127_parent_row_right_click_opens_its_path_menu_without_navigation(cx: &mut TestAppContext) {
    columns_menu(cx, Slot::Parent, false);
}

#[gpui::test]
fn ey127_peeked_child_file_right_click_opens_its_path_menu_without_navigation(
    cx: &mut TestAppContext,
) {
    columns_menu(cx, Slot::Child, false);
}

#[gpui::test]
fn ey127_a_missing_neighbour_row_never_opens_a_later_menu(cx: &mut TestAppContext) {
    columns_menu(cx, Slot::Parent, true);
}
