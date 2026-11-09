//! Trash borrows window controls through the mounted root and retains its own row actions.
use super::join_tests::{answer, call, draw, opened, root, shell};
use crate::list::bench::real_cx;
use crate::window::user_keys::test_fixture::samples;
use flea::jsondoc::Json;
use gpui::{point, px, Modifiers, ScrollDelta, ScrollWheelEvent, TouchPhase, VisualTestContext};

#[test]
fn pb_trash_direct_keys_reach_sheet_path_and_text_size() {
    for key in ["?", "ctrl-l", "ctrl-=", "ctrl--", "ctrl-0"] {
        let mut cx = real_cx();
        let window = root(&mut cx);
        opened(&mut cx, window);
        cx.simulate_keystrokes(window.into(), key);
        shell(&mut cx, window, |shell, _, cx| {
            assert!(shell.trash_opened(cx));
            match key {
                "?" => assert!(shell.settings.sheet.is_some(), "Trash opens the sheet"),
                "ctrl-l" => assert!(shell.chrome.view.as_ref().unwrap().read_with(cx, |v, cx| v
                    .path
                    .read(cx)
                    .entry
                    .editing)),
                _ => assert!(
                    shell
                        .launch
                        .settings
                        .data
                        .get("display")
                        .and_then(|d| d.get("textSize"))
                        .is_some(),
                    "Trash forwards {key}"
                ),
            }
        });
    }
}

#[test]
fn pb_trash_window_keys_reach_settings_sidebar_and_tabs() {
    for key in [",", "ctrl-b", "t"] {
        let mut cx = real_cx();
        let window = root(&mut cx);
        shell(&mut cx, window, |shell, _, cx| shell.mount_rail(cx));
        opened(&mut cx, window);
        cx.simulate_keystrokes(window.into(), key);
        shell(&mut cx, window, |shell, _, _| match key {
            "," => assert!(shell.settings.opened),
            "ctrl-b" => assert_eq!(
                shell
                    .launch
                    .settings
                    .data
                    .get("places")
                    .and_then(|v| v.get("rail"))
                    .and_then(Json::as_str),
                Some("hidden")
            ),
            _ => assert_eq!(shell.strip.tabs[0].count(), 2),
        });
    }
}

#[test]
fn pb_trash_reverse_tab_focuses_empty_and_return_reviews_every_item() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "shift-tab");
    shell(&mut cx, window, |shell, _, cx| {
        assert!(
            shell
                .trash
                .as_ref()
                .unwrap()
                .pane
                .read(cx)
                .view()
                .strip_focus
        );
    });
    cx.simulate_keystrokes(window.into(), "enter");
    shell(&mut cx, window, |shell, _, cx| {
        let view = shell.trash.as_ref().unwrap().pane.read(cx).view();
        assert_eq!(view.pending_op, "prepare");
        assert!(view.confirming);
    });
    answer(
        &mut cx,
        window,
        "prepare",
        r#", "token":91,"count":2,"bytes":11,"all":true"#,
    );
    draw(&mut cx, window);
    assert!(call(&mut cx, window, "trashState", &[]).contains("Empty Trash?"));
}

#[test]
fn pb_trash_scrollbar_shows_on_use_and_lane_moves_only_trash() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, cx| pane.action("reload", false, cx));
    });
    let rows = (0..100)
        .map(|i| {
            format!(r#"{{"uri":"trash:///{i}","original":"/fixture/{i}","identity":"id-{i}"}}"#)
        })
        .collect::<Vec<_>>()
        .join(",");
    answer(
        &mut cx,
        window,
        "list",
        &format!(r#", "total":100,"start":0,"rows":[{rows}]"#),
    );
    answer(&mut cx, window, "summary", r#", "bytes":0,"partial":false"#);
    draw(&mut cx, window);
    let bounds = shell(&mut cx, window, |shell, _, cx| {
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .read(cx)
            .paint
            .borrow()
            .bounds
            .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.simulate_event(ScrollWheelEvent {
        position: bounds.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-31.))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    draw(&mut cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    assert!(visual.debug_bounds("trash-scroll-knob").is_some());
    visual.simulate_click(
        point(bounds.right() - px(3.), bounds.bottom() - px(5.)),
        Modifiers::none(),
    );
    shell(&mut cx, window, |shell, _, cx| {
        assert!(shell.trash.as_ref().unwrap().pane.read(cx).view().scroll_y > 31.);
        assert!(shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .read(cx)
            .scrollbar
            .model
            .drag
            .is_none());
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.scroll_y, 0.);
    });
    draw(&mut cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    let knob = visual.debug_bounds("trash-scroll-knob").unwrap();
    visual.simulate_mouse_down(knob.center(), gpui::MouseButton::Left, Modifiers::none());
    let destination = point(knob.center().x, bounds.center().y);
    visual.simulate_mouse_move(
        destination,
        Some(gpui::MouseButton::Left),
        Modifiers::none(),
    );
    visual.simulate_mouse_up(destination, gpui::MouseButton::Left, Modifiers::none());
    shell(&mut cx, window, |shell, _, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        assert!(pane.scrollbar.model.drag.is_none());
        assert!(
            pane.view().scroll_y
                < 100. * super::board::Board::default().row_h - pane.view().height - 5.
        );
    });
    draw(&mut cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    let knob = visual.debug_bounds("trash-scroll-knob").unwrap();
    visual.simulate_mouse_down(knob.center(), gpui::MouseButton::Left, Modifiers::none());
    let outside = point(bounds.left() - px(10.), bounds.top() + px(5.));
    visual.simulate_mouse_move(outside, Some(gpui::MouseButton::Left), Modifiers::none());
    visual.simulate_mouse_up(outside, gpui::MouseButton::Left, Modifiers::none());
    shell(&mut cx, window, |shell, _, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        assert!(pane.scrollbar.model.drag.is_none());
        assert_eq!(pane.view().scroll_y, 0.);
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.scroll_y, 0.);
    });
}

// EY-310: Trash's sheet lists the listing keys Trash answers itself, and a sheet row runs in Trash.
#[test]
fn ey310_trash_sheet_lists_and_runs_the_keys_trash_answers() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    cx.simulate_keystrokes(window.into(), "?");
    shell(&mut cx, window, |shell, _, cx| {
        let sheet = &shell.settings.sheet.as_ref().unwrap().read(cx).sheet;
        let drawn: Vec<&str> = sheet
            .groups()
            .iter()
            .flat_map(|(_, _, rows)| rows.iter().map(|row| row.action))
            .collect();
        for own in [
            "cursorDown",
            "cursorUp",
            "cursorFirst",
            "cursorLast",
            "toggleSelect",
            "selectAll",
            "deletePermanently",
            "tabNew",
            "keymapSheet",
        ] {
            assert!(
                drawn.contains(&own),
                "the Trash sheet draws {own}: {drawn:?}"
            );
        }
        for foreign in ["copy", "paste", "rename", "trash", "open", "filter"] {
            assert!(
                !drawn.contains(&foreign),
                "the Trash sheet leaves out {foreign}"
            );
        }
    });
    cx.simulate_keystrokes(window.into(), "l a s t space i t e m enter");
    cx.run_until_parked();
    shell(&mut cx, window, |shell, window, cx| {
        assert!(shell.settings.sheet.is_none());
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        assert_eq!(
            pane.view().cursor,
            1,
            "Last item from the sheet moves Trash's cursor"
        );
        assert!(pane.focus.is_focused(window));
    });
}

#[test]
fn pb_trash_sheet_has_only_borrowed_keys_and_its_live_trash_menu() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        shell.open_sheet(window, cx)
    });
    shell(&mut cx, window, |shell, _, cx| {
        let sheet = &shell.settings.sheet.as_ref().unwrap().read(cx).sheet;
        assert!(sheet
            .rows
            .iter()
            .all(|row| flea::ui_logic_pl3::host::sheet_action(row.action)));
        let menus = &sheet.sources.menus;
        assert!(menus
            .iter()
            .any(|row| row.action == "restoreTrashSelection" && !row.disabled));
        assert!(menus
            .iter()
            .any(|row| row.action == "emptyTrash" && row.danger));
        assert!(!menus.iter().any(|row| row.action == "paste"));
    });
    cx.simulate_keystrokes(window.into(), "r e s t o r e enter");
    cx.run_until_parked();
    shell(&mut cx, window, |shell, window, cx| {
        assert!(shell.settings.sheet.is_none());
        let pane = shell.trash.as_ref().unwrap().pane.read(cx);
        assert_eq!(pane.view().pending_op, "restore");
        assert!(pane.focus.is_focused(window));
        assert_eq!(pane.view().selection.uris(), ["trash:///alpha"]);
    });
}

// Over Trash the sheet's keyboard lights only the keys Trash answers, and its search finds key bindings only.
#[test]
fn trash_sheet_keyboard_shows_the_keys_trash_answers() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        shell.open_sheet(window, cx);
        let keyboard = shell
            .sheet_keyboard_from("default", false, samples)
            .map(|keyboard| shell.over_trash(keyboard, cx));
        let bound: Vec<String> = keyboard
            .iter()
            .flat_map(|k| &k.bound)
            .map(|b| b.action.clone())
            .collect();
        assert!(
            bound.iter().any(|action| action == "toggleSelect"),
            "{bound:?}"
        );
        assert!(
            !bound.iter().any(|action| action == "copy"),
            "Trash does not copy"
        );
        assert!(
            bound
                .iter()
                .all(|action| flea::ui_logic_pl3::host::sheet_action(action)),
            "{bound:?}"
        );
        let view = shell.settings.sheet.clone().expect("sheet opened");
        view.update(cx, |view, _| view.sheet.set_keyboard(keyboard));
    });
    cx.simulate_keystrokes(window.into(), "ctrl-f r e s t o r e");
    shell(&mut cx, window, |shell, _, cx| {
        let sheet = &shell
            .settings
            .sheet
            .as_ref()
            .expect("sheet open")
            .read(cx)
            .sheet;
        assert!(sheet.typing, "Ctrl+F opened the search");
        assert_eq!(sheet.query, "restore", "the search took the word");
        assert!(
            sheet.ranked.iter().all(|row| row.menu_action.is_empty()),
            "no Trash menu rows"
        );
    });
}
