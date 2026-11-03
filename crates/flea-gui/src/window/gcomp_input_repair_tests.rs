//! Regressions deliver platform events through the mounted Root and its real focus owners.
use super::super::*;
use gpui::{
    point, px, Bounds, Modifiers, Pixels, Point, ScrollDelta, ScrollWheelEvent, VisualTestContext,
};

fn key(cx: &mut TestAppContext, handle: Handle, value: &str) {
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(value);
    frame(cx, handle);
}

fn wheel(cx: &mut TestAppContext, handle: Handle, at: Point<Pixels>, delta: ScrollDelta) {
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_move(at, None, Modifiers::none());
    visual.simulate_event(ScrollWheelEvent {
        position: at,
        delta,
        ..Default::default()
    });
    frame(cx, handle);
}

#[gpui::test]
fn g70_dual_escape_returns_from_tabbed_chrome(cx: &mut TestAppContext) {
    let handle = open(cx);
    for preset in flea::keys::PRESETS {
        for side in 0..2 {
            edit(cx, handle, |shell, window, cx| {
                let patch = flea::jsondoc::parse(&format!(
                    r#"{{"keys":"{preset}","places":{{"rail":"hidden"}}}}"#
                ))
                .unwrap();
                shell.launch.settings.data =
                    flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
                for host in shell.panes.iter_mut().flatten() {
                    host.pane.cursor = 0;
                    host.selected = vec![0, 2];
                }
                shell.focus_pane(side, cx);
                window.focus(&shell.focus, cx);
                shell.keyboard.context = flea::keys::Context::Listing;
            });
            frame(cx, handle);
            key(cx, handle, if side == 0 { "shift-tab" } else { "tab" });
            edit(cx, handle, |shell, window, cx| {
                assert!(
                    shell.dual_focus.chrome.contains_focused(window, cx),
                    "G70_TAB_REACHED_CHROME"
                );
                let chrome = shell.chrome.view.as_ref().unwrap().read(cx);
                let glyph = if side == 0 { 9 } else { 1 };
                assert!(chrome.focus[glyph].is_focused(window));
                assert_eq!(shell.keyboard.context, flea::keys::Context::Panel);
                assert_eq!(shell.focus_side, side);
            });
            key(cx, handle, "down");
            edit(cx, handle, |shell, _, _| {
                for host in shell.panes.iter().flatten() {
                    assert_eq!(host.pane.cursor, 0, "G70_PANEL_DOWN_KEEPS_LISTING_IDLE");
                }
                shell.keyboard.set_composing(true);
            });
            key(cx, handle, "escape");
            edit(cx, handle, |shell, window, cx| {
                assert!(
                    shell.dual_focus.chrome.contains_focused(window, cx),
                    "G70_COMPOSITION_OWNS_ESCAPE"
                );
                shell.keyboard.set_composing(false);
            });
            key(cx, handle, "escape");
            edit(cx, handle, |shell, window, _| {
                assert!(
                    shell.focus.is_focused(window),
                    "G70_ESCAPE_RETURNS_TO_LISTING"
                );
                assert_eq!(shell.keyboard.context, flea::keys::Context::Listing);
                assert_eq!(shell.focus_side, side, "G70_ESCAPE_RETAINS_PANE");
                for host in shell.panes.iter().flatten() {
                    assert_eq!(
                        host.selected,
                        [0, 2],
                        "G70_FOCUS_ESCAPE_PRESERVES_SELECTION"
                    );
                }
            });
            key(cx, handle, "escape");
            edit(cx, handle, |shell, _, _| {
                assert!(
                    shell.panes[side].as_ref().unwrap().selected.is_empty(),
                    "G70_LISTING_ESCAPE_CLEARS_SELECTION"
                );
                assert_eq!(shell.panes[1 - side].as_ref().unwrap().selected, [0, 2]);
            });
            key(cx, handle, "down");
            edit(cx, handle, |shell, _, _| {
                for (at, host) in shell.panes.iter().enumerate() {
                    assert_eq!(
                        host.as_ref().unwrap().pane.cursor,
                        usize::from(at == side),
                        "G70_DOWN_REACHES_RETURNED_PANE"
                    );
                }
            });
        }
    }
}

#[gpui::test]
fn g70_dual_escape_preserves_path_and_modal_owners(cx: &mut TestAppContext) {
    let handle = open(cx);
    edit(cx, handle, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 2];
        shell.chrome_action("pathBar", window, cx);
    });
    frame(cx, handle);
    edit(cx, handle, |shell, window, cx| {
        assert!(
            shell.path_holds_keyboard(window, cx),
            "G70_PATH_OWNS_KEYBOARD"
        );
    });
    key(cx, handle, "escape");
    edit(cx, handle, |shell, _, cx| {
        let path = shell
            .chrome
            .view
            .as_ref()
            .unwrap()
            .read_with(cx, |chrome, _| chrome.path.clone());
        assert!(
            !path.read_with(cx, |path, _| path.entry.editing),
            "G70_PATH_ESCAPE_CLOSES_EDITOR"
        );
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0, 2]);
    });
    let overlay = edit(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/fx/v/work", window, cx);
        shell.overlay.clone().unwrap()
    });
    frame(cx, handle);
    edit(cx, handle, |_, _, cx| {
        assert!(
            overlay.read_with(cx, |card, _| card.model.open()),
            "G70_MODAL_IS_OPEN"
        );
    });
    key(cx, handle, "escape");
    edit(cx, handle, |shell, _, cx| {
        assert!(
            !overlay.read_with(cx, |card, _| card.model.open()),
            "G70_MODAL_ESCAPE_CLOSES_CARD"
        );
        assert_eq!(
            shell.panes[0].as_ref().unwrap().selected,
            [0, 2],
            "G70_MODAL_ESCAPE_PRESERVES_SELECTION"
        );
    });
}

fn grid(cx: &mut TestAppContext) -> Handle {
    let handle = open(cx);
    edit(cx, handle, |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        assert!(pane.begin("/files".into(), Default::default(), pane.now()));
        for reply in [
            flea::protocol::Reply::Listed {
                path: "/files".into(),
                total: 200,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                listing: 4,
                rows: (0..200)
                    .map(|at| flea::protocol::RowFacts {
                        name: format!("file-{at}.txt"),
                        mode: 0o100644,
                        ..Default::default()
                    })
                    .collect(),
            },
        ] {
            shell.receive(0, BackendEvent::Reply(reply), window, cx);
        }
        shell.set_view(View::Grid, window, cx);
    });
    frame(cx, handle);
    bounds(cx, handle);
    handle
}

fn bounds(cx: &mut TestAppContext, handle: Handle) -> Bounds<Pixels> {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.drawn_view, Some(View::Grid), "G70_REAL_GRID_MOUNTED");
        let grid = host.grid.as_ref().unwrap();
        let state = grid.state.borrow();
        assert!(!state.drawn().is_empty(), "G70_REAL_GRID_PAINTED");
        assert!(
            state.layout.unwrap().max_scroll() > 500.,
            "G70_FIXTURE_CAN_SCROLL"
        );
        state.bounds
    })
}

fn scroll(cx: &mut TestAppContext, handle: Handle) -> f32 {
    edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .grid
            .as_ref()
            .unwrap()
            .view
            .scroll
    })
}

fn excluded(cx: &mut TestAppContext, status: bool) {
    let handle = grid(cx);
    for delta in [
        ScrollDelta::Lines(point(0., -1.)),
        ScrollDelta::Pixels(point(px(0.), px(-13.))),
    ] {
        let at = edit(cx, handle, |shell, window, _| {
            let geometry = shell.geometry(window);
            let area = if status {
                geometry.status
            } else {
                geometry.chrome
            };
            point(px(area.x + area.width / 2.), px(area.y + area.height / 2.))
        });
        assert!(
            !bounds(cx, handle).contains(&at),
            "G70_TARGET_OUTSIDE_PAINTED_GRID"
        );
        let before = scroll(cx, handle);
        wheel(cx, handle, at, delta);
        assert_eq!(
            scroll(cx, handle),
            before,
            "{}",
            if status {
                "G70_STATUS_WHEEL_DOES_NOT_SCROLL_GRID"
            } else {
                "G70_CHROME_WHEEL_DOES_NOT_SCROLL_GRID"
            }
        );
        let center = bounds(cx, handle).center();
        wheel(cx, handle, center, delta);
        assert!(scroll(cx, handle) > before, "G70_PANE_WHEEL_STILL_SCROLLS");
    }
}

#[gpui::test]
fn g70_grid_wheel_excludes_chrome(cx: &mut TestAppContext) {
    excluded(cx, false);
}

#[gpui::test]
fn g70_grid_wheel_excludes_status(cx: &mut TestAppContext) {
    excluded(cx, true);
}

#[gpui::test]
fn g70_grid_wheel_uses_painted_pane_edges(cx: &mut TestAppContext) {
    let handle = grid(cx);
    for size in [
        gpui::size(px(640.), px(420.)),
        gpui::size(px(900.), px(600.)),
    ] {
        VisualTestContext::from_window(handle.into(), cx).simulate_resize(size);
        frame(cx, handle);
        let bounds = bounds(cx, handle);
        assert!(
            bounds.left() > px(0.),
            "G70_DOCKED_RAIL_LEAVES_LEFT_BOUNDARY"
        );
        let center = bounds.center();
        for at in [
            point(bounds.left() - px(0.5), center.y),
            point(center.x, bounds.top() - px(0.5)),
            point(center.x, bounds.bottom()),
        ] {
            let before = scroll(cx, handle);
            wheel(cx, handle, at, ScrollDelta::Lines(point(0., -1.)));
            assert_eq!(
                scroll(cx, handle),
                before,
                "G70_OUTSIDE_PANE_EDGE_STAYS_IDLE: {at:?}"
            );
        }
        for at in [
            point(bounds.left() + px(0.5), center.y),
            point(bounds.right() - px(0.5), center.y),
            point(center.x, bounds.top() + px(0.5)),
            point(center.x, bounds.bottom() - px(0.5)),
        ] {
            let before = scroll(cx, handle);
            wheel(cx, handle, at, ScrollDelta::Lines(point(0., -1.)));
            assert!(
                scroll(cx, handle) > before,
                "G70_INSIDE_PANE_EDGE_SCROLLS: {at:?}"
            );
        }
    }
}

#[gpui::test]
fn g70_grid_menu_wheel_and_escape_keep_their_owners(cx: &mut TestAppContext) {
    let handle = grid(cx);
    let (backend, mut requests) = Backend::test_lines();
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.backend = Some(backend);
        host.selected = vec![0];
    });
    key(cx, handle, "m");
    let id = loop {
        let line = requests().expect("G70_MENU_REQUESTED_SNAPSHOT");
        let document = flea::jsondoc::parse(&line).unwrap();
        if document.get("op").and_then(Json::as_str) == Some("snapshot") {
            break flea::json::field_usize(&line, "id").unwrap();
        }
    };
    for operation in ["snapshot", "applications"] {
        let line = format!(
            r#"{{"t":"menuaction","id":{id},"op":"{operation}","ok":true,"kinds":[32768],"applications":[]}}"#
        );
        edit(cx, handle, |shell, window, cx| {
            shell.receive(0, Backend::test_reply_line(&line), window, cx);
        });
    }
    frame(cx, handle);
    assert_eq!(call(cx, handle, "contextMenuCursor"), Reply::value("0"));
    let (at, expected) = edit(cx, handle, |shell, _, _| {
        let menu = shell.menu.menu.borrow();
        let card = menu.card.as_ref().expect("G70_REAL_MENU_MOUNTED");
        // Keep the helper's mouse move on row zero before the one-notch wheel.
        let at = point(
            px(card.frame.x + card.frame.w / 2.),
            px(card.row_screen_top(0) + flea::ui_logic_mn1::fit::ROW as f32 / 2.),
        );
        assert_eq!(
            card.row_at(f32::from(at.x), f32::from(at.y)),
            Some(0),
            "G70_MENU_WHEEL_TARGETS_FIRST_ROW"
        );
        let from = card.cursor.expect("G70_MENU_HAS_CURSOR");
        let expected = card
            .entries
            .iter()
            .enumerate()
            .skip(from + 1)
            .find_map(|(index, entry)| (!entry.separator && !entry.disabled).then_some(index))
            .expect("G70_MENU_HAS_NEXT_ENABLED_ROW");
        assert!(expected > from, "G70_MENU_EXPECTS_FORWARD_MOVEMENT");
        (at, expected)
    });
    let before = scroll(cx, handle);
    wheel(cx, handle, at, ScrollDelta::Lines(point(0., -3.)));
    assert_eq!(
        call(cx, handle, "contextMenuCursor"),
        Reply::value(expected.to_string()),
        "G70_MENU_WHEEL_ADVANCES_TO_NEXT_ENABLED_ROW"
    );
    assert_eq!(
        scroll(cx, handle),
        before,
        "G70_MENU_WHEEL_LEAVES_GRID_STILL"
    );
    key(cx, handle, "escape");
    assert_eq!(
        call(cx, handle, "contextMenuVisible"),
        Reply::value("false")
    );
    edit(cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().selected,
            [0],
            "G70_MENU_ESCAPE_PRESERVES_SELECTION"
        );
    });
}
