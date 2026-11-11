//! U-MENU-REFUSE: rows walked with the keys and chosen with Return reach their owner, never "no longer available".
use super::super::test_shell::{sent, set_cursor};
use super::*;

// Every row shown, as the sweep's menu scenes run, with the cursor on notes.txt.
fn on_notes(cx: &mut gpui::TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open(cx, "default");
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.launch.settings.data =
                jsondoc::parse(r#"{"keys":"default","view":"list","menu":{"hidden":[]}}"#).unwrap();
        })
    });
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    (shell, cx)
}

// The row menu's snapshot, and an application producer that offers one handler, so Open With draws its flyout.
fn answer_with_app(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let id = captured(shell, cx);
    deliver(shell, cx, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[{FILE}]}}"));
    reply(shell, cx, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"applications\",\"ok\":true,\"applications\":[{{\"id\":\"nvim.desktop\",\"label\":\"Neovim\",\"icon\":\"nvim\",\"default\":true}}]}}"));
}

fn sent_json() -> Vec<Json> {
    sent()
        .iter()
        .filter_map(|line| jsondoc::parse(line).ok())
        .collect()
}

#[gpui::test]
fn fxuish_def2_favourite_discovery_preserves_the_validation_refusal(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[DIR]);
    let id = captured(&shell, cx);
    shell.update(cx, |shell, _| shell.menu.reading = true);
    choose(&shell, cx, "addFavourite");
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"validate","action":"__menuScripts","ok":false,"error":"Selected item changed; reopen the menu."}}"#
        ),
    );
    assert_eq!(
        status(&shell, cx),
        "Selected item changed; reopen the menu."
    );
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "Selected item changed; reopen the menu."
    );
    assert!(!shell.read_with(cx, |shell, _| shell.menu_opened()));
    assert!(shell.read_with(cx, |shell, _| shell
        .launch
        .settings
        .data
        .get("places")
        .and_then(|places| places.get("favourites"))
        .and_then(Json::as_array)
        .is_none_or(|favourites| favourites.is_empty())));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn fxuish_def2_favourite_listing_change_refuses_every_stale_menu_path(
    cx: &mut gpui::TestAppContext,
) {
    for path in [
        "choose", "refresh", "reply", "reread", "moved", "gone", "detail",
    ] {
        let (shell, cx) = open(cx, "default");
        land(&shell, cx, "/files", home(), 1);
        cx.simulate_keystrokes("m");
        answer(&shell, cx, &[DIR]);
        let id = captured(&shell, cx);
        if matches!(path, "reply" | "detail") {
            choose(&shell, cx, "addFavourite");
        }
        if path == "reread" {
            cx.update(|window, cx| {
                shell.update(cx, |shell, cx| {
                    shell
                        .step(0, crate::window::navigate::Step::Refresh(None), window, cx)
                        .unwrap();
                })
            });
        } else if path != "detail" {
            shell.update(cx, |shell, _| {
                let host = shell.panes[0].as_mut().unwrap();
                host.pane.held = std::rc::Rc::new(crate::list::pane::Held {
                    listing: host.pane.held.listing + 1,
                    total: host.pane.held.total,
                    start: host.pane.held.start,
                    rows: host.pane.held.rows.clone(),
                });
                if path == "moved" {
                    host.path = "/elsewhere".into();
                }
                if path == "gone" {
                    host.pane.held = Default::default();
                }
            });
        }
        // A late reply rejects its stale listing before detail; a current capture takes the detail branch.
        match path {
            "choose" | "reread" | "moved" | "gone" => choose(&shell, cx, "addFavourite"),
            "refresh" => {
                cx.update(|window, cx| shell.update(cx, |shell, cx| shell.refresh_menu(window, cx)))
            }
            _ => reply(
                &shell,
                cx,
                &format!(
                    r#"{{"t":"menuaction","id":{id},"op":"validate","action":"addFavourite","ok":false,"error":"The folder was replaced; reopen the menu."}}"#
                ),
            ),
        }
        let expected = match path {
            "detail" => "The folder was replaced; reopen the menu.",
            "moved" | "gone" => "Selected item moved or disappeared; reopen the menu.",
            _ => "Selected item changed; reopen the menu.",
        };
        assert_eq!(status(&shell, cx), expected, "{path}");
        assert_eq!(read(&shell, cx, "lastMessage", &[]), expected, "{path}");
        assert!(
            !shell.read_with(cx, |shell, _| shell.menu_opened()),
            "{path}"
        );
        assert!(shell.read_with(cx, |shell, _| shell
            .launch
            .settings
            .data
            .get("places")
            .and_then(|places| places.get("favourites"))
            .and_then(Json::as_array)
            .is_none_or(|favourites| favourites.is_empty())));
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn fxuish_def2_replaced_favourite_preserves_the_identity_refusal(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[DIR]);
    let id = captured(&shell, cx);
    choose(&shell, cx, "addFavourite");
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"validate","action":"addFavourite","ok":false,"error":"The folder was replaced; reopen the menu."}}"#
        ),
    );
    assert_eq!(
        status(&shell, cx),
        "The folder was replaced; reopen the menu."
    );
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "The folder was replaced; reopen the menu."
    );
    assert!(!shell.read_with(cx, |shell, _| shell.menu_opened()));
    assert!(shell.read_with(cx, |shell, _| shell
        .launch
        .settings
        .data
        .get("places")
        .and_then(|places| places.get("favourites"))
        .and_then(Json::as_array)
        .is_none_or(|favourites| favourites.is_empty())));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// SW-03, EY-196 and EY-293: Open With > Another Application opens the Open With card on the row.
#[gpui::test]
fn open_with_another_application_opens_the_card(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = on_notes(cx);
    cx.simulate_keystrokes("m");
    answer_with_app(&shell, cx);
    choose(&shell, cx, "openWith");
    // The flyout lands on Neovim; Down passes the separator to Another Application.
    cx.simulate_keystrokes("down enter");
    settle(cx);
    validated(&shell, cx, "openWith:__another__", &["/files/notes.txt"]);
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    assert!(
        shell.read_with(cx, |shell, _| shell.open_with.is_some()),
        "the Open With card opens"
    );
    assert!(!shell.read_with(cx, |shell, _| shell.menu_opened()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// PaneMenuActions.qml: a flyout application launches under the menu's id, and a refused launch says why.
#[gpui::test]
fn open_with_an_application_launches_it(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = on_notes(cx);
    cx.simulate_keystrokes("m");
    answer_with_app(&shell, cx);
    let id = captured(&shell, cx);
    choose(&shell, cx, "openWith");
    cx.simulate_keystrokes("enter");
    settle(cx);
    sent();
    validated(&shell, cx, "openWith:nvim.desktop", &["/files/notes.txt"]);
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    let launch = sent_json();
    assert!(
        launch.iter().any(|line| {
            line.get("op").and_then(Json::as_str) == Some("openWith")
                && line.get("application").and_then(Json::as_str) == Some("nvim.desktop")
                && line.get("id").and_then(Json::as_f64) == Some(id as f64)
        }),
        "the launch goes out under the menu id: {launch:?}"
    );
    assert!(
        !launch
            .iter()
            .any(|line| line.get("op").and_then(Json::as_str) == Some("close")),
        "the snapshot stays open for the launch: {launch:?}"
    );
    reply(
        &shell,
        cx,
        &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"openWith\",\"ok\":false,\"error\":\"Neovim refused.\"}}"),
    );
    assert_eq!(status(&shell, cx), "Neovim refused.");
    assert!(
        sent_json().iter().any(|line| {
            line.get("op").and_then(Json::as_str) == Some("close")
                && line.get("id").and_then(Json::as_f64) == Some(id as f64)
        }),
        "the answer closes the snapshot"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// EY-164: c opens Copy As on the cursor row; Down twice past Relative Path and Return choose Name, which lands on the clipboard.
#[gpui::test]
fn copy_as_name_with_return_copies_it(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = on_notes(cx);
    cx.simulate_keystrokes("c");
    answer(&shell, cx, &[FILE]);
    cx.simulate_keystrokes("down down enter");
    settle(cx);
    validated(&shell, cx, "copyAs:copyName", &["/files/notes.txt"]);
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("notes.txt")
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// EY-175: New File on the background menu opens its name card.
#[gpui::test]
fn new_file_from_the_background_opens_its_card(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = on_notes(cx);
    right_click(cx, 600., 500.);
    choose(&shell, cx, "newFile");
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    let opened = cx.update(|_, cx| {
        shell.read(cx).operation_view(0).is_some_and(|view| {
            matches!(
                view.read(cx).model.dialog,
                Some(crate::g2::file_ops::model::Dialog::NewFile { .. })
            )
        })
    });
    assert!(opened, "the New File card opens");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// EY-175: Show Original on a link asks the backend for its target.
#[gpui::test]
fn show_original_asks_for_the_target(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = open(cx, "default");
    let mut rows = home();
    rows.push(RowFacts {
        link: Some("notes.txt".into()),
        ..entry("link.txt", 0o120777)
    });
    land(&shell, cx, "/files", rows, 1);
    set_cursor(&shell, cx, 4);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[0o120777]);
    choose(&shell, cx, "showOriginal");
    sent();
    validated(&shell, cx, "showOriginal", &["/files/link.txt"]);
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    assert!(
        sent_json()
            .iter()
            .any(|line| line.get("c").and_then(Json::as_str) == Some("linktarget")),
        "the target is asked for"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// EY-192: Permissions… with notes.txt and todo.md marked opens the card for both.
#[gpui::test]
fn permissions_for_two_marked_files_opens_the_card(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = on_notes(cx);
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.panes[0].as_mut().unwrap().selected = vec![2, 3]
        })
    });
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[FILE, FILE]);
    choose(&shell, cx, "permissions");
    validated(
        &shell,
        cx,
        "permissions",
        &["/files/notes.txt", "/files/todo.md"],
    );
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    assert!(
        cx.update(|_, cx| shell.read(cx).permissions_showing(cx)),
        "the Permissions card opens"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// EY-195's shelf success cases live in g2/plugins/menu_shelf_join_tests.rs, against the real Shelf provider.

// EY-268: Recent opened from the rail, then the background menu's New File, refuses as New Folder's key does and opens no card.
#[gpui::test]
fn new_file_in_rail_recent_refuses_from_the_menu(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = on_notes(cx);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.recent.on = true;
        host.recent.from = "/files".into();
    });
    right_click(cx, 600., 500.);
    choose(&shell, cx, "newFile");
    let opened = cx.update(|_, cx| {
        shell.read(cx).operation_view(0).is_some_and(|view| {
            matches!(
                view.read(cx).model.dialog,
                Some(crate::g2::file_ops::model::Dialog::NewFile { .. })
            )
        })
    });
    assert!(!opened, "Recent cannot hold a new file, so no card opens");
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "Recent cannot hold a new file. Open a folder first"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// EY-226 (2): with the row menu open on notes.txt and no flyout, the digit 1 tags it Red and the menu stays open.
#[gpui::test]
fn a_digit_in_the_open_row_menu_tags_the_row(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = on_notes(cx);
    cx.simulate_keystrokes("m");
    tags_answer(&shell, cx, &[FILE]);
    sent();
    cx.simulate_keystrokes("1");
    settle(cx);
    assert_ne!(status(&shell, cx), UNAVAILABLE);
    assert_ne!(read(&shell, cx, "lastMessage", &[]), UNAVAILABLE);
    validated(&shell, cx, "tags:tag1", &["/files/notes.txt"]);
    let lines = sent_json();
    let settags = lines
        .iter()
        .find(|line| line.get("c").and_then(Json::as_str) == Some("settags"))
        .unwrap_or_else(|| panic!("no settags sent: {lines:?}"));
    assert_eq!(settags.get("tag").and_then(Json::as_str), Some("Red"));
    assert_ne!(read(&shell, cx, "lastMessage", &[]), UNAVAILABLE);
    assert!(
        shell.read_with(cx, |shell, _| shell.menu_opened()),
        "the menu stays open, as v0.3.12 keeps it"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
