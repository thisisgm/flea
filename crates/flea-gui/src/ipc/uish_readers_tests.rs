//! FX-UISH readers the native ui.sh cases assert on, reached through the production Root dispatch.
use super::readers3_tests::{json, open, update};
use flea::jsondoc::Json;
use gpui::{TestAppContext, VisualTestContext};

fn field<'a>(state: &'a Json, name: &str) -> &'a Json {
    state.get(name).unwrap_or_else(|| panic!("{name} missing"))
}

fn control<'a>(state: &'a Json, name: &str) -> &'a Json {
    let Some(Json::Arr(controls)) = state.get("controls") else {
        panic!("controls missing");
    };
    controls
        .iter()
        .find(|control| control.get("name").and_then(Json::as_str) == Some(name))
        .unwrap_or_else(|| panic!("control {name} missing"))
}

#[gpui::test]
fn fx_uish_menu_dialog_state_describes_the_copy_to_card(cx: &mut TestAppContext) {
    let handle = open(cx);
    assert_eq!(
        field(&json(cx, handle, "menuDialogState"), "opened").as_bool(),
        Some(false)
    );
    update(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files/dest", window, cx)
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let state = json(cx, handle, "menuDialogState");
    assert_eq!(field(&state, "opened").as_bool(), Some(true), "{state:?}");
    assert_eq!(field(&state, "action").as_str(), Some("copyTo"));
    assert_eq!(field(&state, "busy").as_bool(), Some(false));
    assert_eq!(field(&state, "error").as_str(), Some(""));
    assert!(!field(&state, "rect").as_str().unwrap().is_empty());
    assert_eq!(
        field(control(&state, "Field"), "focused").as_bool(),
        Some(true)
    );
    let submit = control(&state, "Submit");
    assert!(!field(control(&state, "Field"), "rect")
        .as_str()
        .unwrap()
        .is_empty());
    assert_eq!(field(submit, "visible").as_bool(), Some(true));
    assert!(!field(submit, "label").as_str().unwrap().is_empty());
    assert!(!field(control(&state, "Cancel"), "centre")
        .as_str()
        .unwrap()
        .is_empty());
    update(cx, handle, |shell, window, cx| {
        shell.close_overlay(window, cx)
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    assert_eq!(
        field(&json(cx, handle, "menuDialogState"), "opened").as_bool(),
        Some(false)
    );
}

#[gpui::test]
fn fx_uish_menu_dialog_state_names_move_to(cx: &mut TestAppContext) {
    let handle = open(cx);
    update(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(true, "/files/dest", window, cx)
    });
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let state = json(cx, handle, "menuDialogState");
    assert_eq!(
        field(&state, "action").as_str(),
        Some("moveTo"),
        "{state:?}"
    );
}

#[gpui::test]
fn fx_uish_a_field_text_menu_counts_as_a_context_menu(cx: &mut TestAppContext) {
    use super::readers3_tests::read;
    let handle = open(cx);
    let other = open(cx);
    let visible = |cx: &mut TestAppContext| read(cx, handle, "contextMenuVisible", &[]).stdout;
    assert_eq!(visible(cx).trim(), "false");
    let catalog = update(cx, handle, |shell, _, _| shell.locale.catalog.clone());
    // The field opens its card outside the Root's own update, as a field view does.
    let mut field = VisualTestContext::from_window(handle.into(), cx).update(|window, cx| {
        let mut menu = crate::path::text_menu::TextMenu::default();
        let entry = crate::path::entry::Entry::default();
        let at = gpui::point(gpui::px(40.), gpui::px(40.));
        menu.open(&entry, &catalog, at, window, cx);
        menu
    });
    assert_eq!(visible(cx).trim(), "true");
    assert_eq!(
        read(cx, other, "contextMenuVisible", &[]).stdout.trim(),
        "false"
    );
    let entries = read(cx, handle, "contextMenuEntries", &[]).stdout;
    assert!(entries.split('|').count() >= 5, "{entries}");
    field.close();
    assert_eq!(visible(cx).trim(), "false");
}

#[gpui::test]
fn fx_uish_rest_readers_answer_trash_rail_search_and_pane_area(cx: &mut TestAppContext) {
    use super::readers3_tests::read;
    let handle = open(cx);
    VisualTestContext::from_window(handle.into(), cx).run_until_parked();
    let trash = json(cx, handle, "trashState");
    assert_eq!(field(&trash, "opened").as_bool(), Some(false));
    let rail = field(&trash, "rail");
    assert_eq!(field(rail, "current").as_bool(), Some(false), "{trash:?}");
    assert_eq!(field(rail, "countText").as_str(), Some(""));
    // An unanswered or refused reader also prints nothing, so each empty answer must be a code 0 reply.
    let mut empty = |name: &str, args: &[&str]| {
        let reply = read(cx, handle, name, args);
        assert_eq!(
            (reply.code, reply.stdout.trim(), reply.stderr.as_str()),
            (0, "", ""),
            "{name}"
        );
    };
    empty("searchMode", &[]);
    empty("searchQuery", &[]);
    empty("searchRect", &[]);
    empty("settingsRailWidth", &[]);
    empty("columnTrackRect", &["active"]);
    empty("paneListAreaRect", &["1"]);
    let left = read(cx, handle, "paneListAreaRect", &["0"]);
    assert_eq!(left.code, 0, "{left:?}");
    assert_eq!(left.stdout.split_whitespace().count(), 4, "{left:?}");
    for side in ["2", "9"] {
        assert_ne!(
            read(cx, handle, "paneListAreaRect", &[side]).code,
            0,
            "{side}"
        );
    }
}

#[test]
fn fx_uish_column_track_reader_uses_the_live_painted_tracks() {
    use crate::window::columns_tests::host;
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, host::settle);
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        let active = host::call(shell, "columnActiveRect", &[], window, cx);
        assert_eq!(active.stdout.split_whitespace().count(), 4);
        assert_eq!(
            host::call(shell, "columnTrackRect", &["active"], window, cx),
            active
        );
        assert!(
            host::call(shell, "columnTrackRect", &["absent"], window, cx)
                .stdout
                .trim()
                .is_empty()
        );
        shell.set_view(crate::window::startup::View::List, window, cx);
        assert!(
            host::call(shell, "columnTrackRect", &["active"], window, cx)
                .stdout
                .trim()
                .is_empty()
        );
    });
}
