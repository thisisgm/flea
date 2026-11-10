//! F3 toggles the dual pane in every preset: on from List, Columns and Grid, and off again to the view it left.
use super::*;
use gpui::VisualTestContext;

fn key(cx: &mut TestAppContext, handle: Handle, stroke: &str) {
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(stroke);
    frame(cx, handle);
}

// The mounted two-pane shell, single in `start` with the list holding the keys, on preset `preset`.
fn single(cx: &mut TestAppContext, preset: &str, start: View) -> Handle {
    let handle = open(cx);
    edit(cx, handle, |shell, window, cx| {
        let mut data = flea::uischema::defaults();
        if let Json::Obj(fields) = &mut data {
            fields.retain(|(key, _)| key != "keys");
            fields.push(("keys".into(), Json::Str(preset.into())));
        }
        shell.launch.settings.data = data;
        shell.set_view(start, window, cx);
        window.focus(&shell.focus, cx);
    });
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.view, start, "{preset}: the start view landed")
    });
    handle
}

fn view(cx: &mut TestAppContext, handle: Handle) -> View {
    edit(cx, handle, |shell, _, _| shell.view)
}

#[gpui::test]
fn chrome_dual_selects_from_every_view_and_keeps_the_keyboard_return_view(cx: &mut TestAppContext) {
    for start in [View::List, View::Columns, View::Grid] {
        let handle = single(cx, "default", start);
        for _ in 0..2 {
            edit(cx, handle, |shell, window, cx| {
                shell.chrome_action("viewDual", window, cx);
            });
            frame(cx, handle);
            assert_eq!(view(cx, handle), View::Dual, "Dual segment from {start:?}");
        }
        key(cx, handle, "f3");
        assert_eq!(view(cx, handle), start, "F3 returns to {start:?}");
        key(cx, handle, "ctrl-4");
        assert_eq!(view(cx, handle), View::Dual);
        key(cx, handle, "ctrl-4");
        assert_eq!(view(cx, handle), start, "Ctrl+4 returns to {start:?}");
    }
}

#[gpui::test]
fn chrome_dual_keeps_a_pending_dual_choice(cx: &mut TestAppContext) {
    let handle = single(cx, "default", View::Columns);
    edit(cx, handle, |shell, window, cx| {
        shell.set_view(View::Dual, window, cx);
        shell.view = View::Columns;
        shell.dual_pending = true;
        shell.chrome_action("viewDual", window, cx);
        assert!(shell.view == View::Dual || shell.dual_pending);
        assert_eq!(shell.dual_focus.before, View::Columns);
    });
    frame(cx, handle);
    assert_eq!(view(cx, handle), View::Dual);
}

#[gpui::test]
fn sheet_dual_row_selects_instead_of_toggling(cx: &mut TestAppContext) {
    let handle = single(cx, "default", View::Columns);
    for _ in 0..2 {
        edit(cx, handle, |shell, window, cx| {
            shell.open_sheet(window, cx);
            shell
                .settings
                .sheet
                .clone()
                .unwrap()
                .update(cx, |view, cx| {
                    view.sheet.decided =
                        Some(flea::ui_logic_settings_pages::sheet::Decision::Action {
                            action: "viewDual".into(),
                        });
                    cx.emit(crate::g2::settings_pages::Request::Close);
                });
        });
        frame(cx, handle);
        assert_eq!(view(cx, handle), View::Dual);
    }
    key(cx, handle, "f3");
    assert_eq!(view(cx, handle), View::Columns);
}

#[gpui::test]
fn f3_enters_dual_and_returns_to_the_view_it_left_in_every_preset(cx: &mut TestAppContext) {
    for preset in flea::keys::PRESETS {
        for start in [View::List, View::Columns, View::Grid] {
            let handle = single(cx, preset, start);
            key(cx, handle, "f3");
            assert_eq!(
                view(cx, handle),
                View::Dual,
                "{preset}: F3 from {start:?} enters Dual"
            );
            key(cx, handle, "f3");
            assert_eq!(
                view(cx, handle),
                start,
                "{preset}: F3 in Dual returns to {start:?}"
            );
            key(cx, handle, "f3");
            assert_eq!(
                view(cx, handle),
                View::Dual,
                "{preset}: F3 enters Dual again"
            );
        }
    }
}

#[gpui::test]
fn the_preset_dual_chord_toggles_the_same_way(cx: &mut TestAppContext) {
    for (preset, chord) in [
        ("default", "ctrl-4"),
        ("vim", "ctrl-4"),
        ("mac", "ctrl-4"),
        ("windows", "ctrl-shift-4"),
    ] {
        let handle = single(cx, preset, View::Columns);
        key(cx, handle, chord);
        assert_eq!(
            view(cx, handle),
            View::Dual,
            "{preset}: {chord} enters Dual"
        );
        key(cx, handle, chord);
        assert_eq!(
            view(cx, handle),
            View::Columns,
            "{preset}: {chord} in Dual returns to Columns"
        );
        key(cx, handle, chord);
        assert_eq!(
            view(cx, handle),
            View::Dual,
            "{preset}: {chord} enters Dual again"
        );
        key(cx, handle, "f3");
        assert_eq!(
            view(cx, handle),
            View::Columns,
            "{preset}: F3 leaves the Dual {chord} opened"
        );
    }
}

#[gpui::test]
fn f3_on_its_way_into_dual_cancels_the_switch(cx: &mut TestAppContext) {
    let handle = single(cx, "default", View::Grid);
    edit(cx, handle, |shell, window, cx| {
        // Rewind to the moment before the secondary's listing landed: the switch is still pending.
        shell.set_view(View::Dual, window, cx);
        shell.launch.settings.data =
            flea::uistate::from_file(r#"{"view":"dual","rememberViewPerFolder":true}"#);
        shell.view = View::Grid;
        shell.dual_pending = true;
    });
    key(cx, handle, "f3");
    edit(cx, handle, |shell, _, _| {
        assert!(!shell.dual_pending, "F3 cancelled the pending switch");
        assert_eq!(shell.view, View::Grid);
        assert_eq!(
            shell
                .launch
                .settings
                .data
                .get("view")
                .and_then(Json::as_str),
            Some("grid"),
            "cancelling pending Dual also updates the standing view"
        );
    });
}

#[gpui::test]
fn f3_in_the_path_field_types_nothing_and_keeps_the_view(cx: &mut TestAppContext) {
    for preset in flea::keys::PRESETS {
        path_field(cx, preset);
    }
}

fn path_field(cx: &mut TestAppContext, preset: &str) {
    let handle = single(cx, preset, View::List);
    key(cx, handle, "ctrl-l");
    let path = edit(cx, handle, |shell, _, cx| {
        shell.chrome.view.as_ref().unwrap().read(cx).path.clone()
    });
    let before = edit(cx, handle, |_, window, cx| {
        let field = path.read(cx);
        assert!(field.holds_keyboard(window), "Ctrl+L opened the path field");
        field.entry.text.clone()
    });
    key(cx, handle, "f3");
    edit(cx, handle, |shell, window, cx| {
        let field = path.read(cx);
        assert!(field.holds_keyboard(window), "F3 left the path field open");
        assert_eq!(field.entry.text, before, "F3 typed into the path field");
        assert_eq!(shell.view, View::List, "F3 in the path field toggled Dual");
        assert!(!shell.dual_pending);
    });
}

#[gpui::test]
fn f3_is_listed_beside_dual_pane_in_every_preset(cx: &mut TestAppContext) {
    for preset in flea::keys::PRESETS {
        let handle = single(cx, preset, View::List);
        edit(cx, handle, |shell, window, cx| {
            shell.open_sheet(window, cx);
            let sheet = &shell.settings.sheet.as_ref().unwrap().read(cx).sheet;
            let rows: Vec<_> = sheet
                .rows
                .iter()
                .filter(|row| row.action == "viewDual")
                .collect();
            assert_eq!(rows.len(), 1, "{preset}: Dual pane has one sheet row");
            assert!(
                rows[0].keys.split(" / ").any(|key| key == "f3"),
                "{preset}: {}",
                rows[0].keys
            );
        });
    }
}

#[gpui::test]
fn f3_in_the_filter_line_types_nothing_and_keeps_the_view(cx: &mut TestAppContext) {
    let handle = single(cx, "default", View::List);
    key(cx, handle, "/");
    key(cx, handle, "a");
    key(cx, handle, "f3");
    let state = json(cx, handle, "keyDeliveryState");
    assert_eq!(state.get("filterTyping"), Some(&Json::Bool(true)));
    assert_eq!(state.get("filterQuery"), Some(&Json::Str("a".into())));
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.view, View::List, "F3 in the filter line toggled Dual");
        assert!(!shell.dual_pending);
    });
}
