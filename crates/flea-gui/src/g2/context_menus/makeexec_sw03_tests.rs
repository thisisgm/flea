//! SW-03: the file menu's rows reach their owners, and the opener rows launch v0.3.12's argv.
use super::fixture::{number, update, Fixture};
use crate::g2::context_menus::join_tests::read;
use flea::jsondoc::{self, Json};
use gpui::TestAppContext;

// SW-03: each row reaches an owner; single-item Rename keeps its snapshot for commit, as Pane.qml onChosen does.
#[gpui::test]
fn sw03_file_menu_rows_reach_an_owner(cx: &mut TestAppContext) {
    let mut refused = Vec::new();
    for action in [
        "cut",
        "copy",
        "duplicate",
        "rename",
        "trash",
        "deletePermanently",
        "openTerminal",
        "moveTo",
        "copyTo",
        "newFile",
        "openAsText",
        "run",
    ] {
        let mut fixture = Fixture::new(cx, 0o644);
        update(cx, fixture.handle, |shell, _, _| {
            shell.launch.settings.data =
                jsondoc::parse(r#"{"menu":{"hidden":["openAgent","actions","runScript"]}}"#)
                    .unwrap();
        });
        // New File is a blank-space row: its menu is the background one, and the card claims it without a validate.
        if action == "newFile" {
            fixture.open_background(cx);
            update(cx, fixture.handle, |shell, window, cx| {
                shell.menu_outcome(
                    crate::g2::context_menus::menu::Outcome::Fire {
                        action: action.into(),
                        leaf: String::new(),
                    },
                    window,
                    cx,
                )
            });
            let said = read(cx, fixture.handle, "lastMessage").stdout;
            let opened = update(cx, fixture.handle, |shell, _, cx| {
                shell.operation_view(0).is_some_and(|view| {
                    matches!(
                        view.read(cx).model.dialog,
                        Some(crate::g2::file_ops::model::Dialog::NewFile { .. })
                    )
                })
            });
            if said.contains("no longer available") || !opened {
                refused.push(action);
            }
            continue;
        }
        fixture.open(cx, 1, vec![]);
        update(cx, fixture.handle, |shell, window, cx| {
            shell.menu_outcome(
                crate::g2::context_menus::menu::Outcome::Fire {
                    action: action.into(),
                    leaf: String::new(),
                },
                window,
                cx,
            )
        });
        let requests = fixture.drain();
        if action == "rename" {
            assert!(requests
                .iter()
                .all(|r| r.get("op").and_then(Json::as_str) != Some("validate")));
            assert_eq!(
                read(cx, fixture.handle, "renameEditorLive").stdout.trim(),
                "true"
            );
            assert!(update(cx, fixture.handle, |shell, _, _| {
                shell.menu.captured.is_none()
                    && shell
                        .rename
                        .edit
                        .as_ref()
                        .is_some_and(|edit| edit.menu != 0)
            }));
            continue;
        }
        let Some(validate) = requests
            .iter()
            .find(|r| r.get("op").and_then(Json::as_str) == Some("validate"))
        else {
            refused.push(action);
            continue;
        };
        fixture.receive(cx, &format!("{{\"t\":\"menuaction\",\"id\":{},\"op\":\"validate\",\"ok\":true,\"action\":\"{action}\",\"paths\":[\"{}\"]}}", number(validate), flea::json::escape(&fixture.root.join("notes.txt").to_string_lossy())));
        cx.run_until_parked();
        let said = read(cx, fixture.handle, "lastMessage").stdout;
        let unclaimed = update(cx, fixture.handle, |shell, _, _| {
            shell.menu.captured.is_some()
        });
        if said.contains("no longer available") || unclaimed {
            refused.push(action);
        }
    }
    assert!(refused.is_empty(), "refused as unavailable: {refused:?}");
}

static LAUNCHED: std::sync::Mutex<Vec<Vec<String>>> = std::sync::Mutex::new(Vec::new());

// Stands in for the opener lane's process launch: records the argv and fails it, so the failure sentence shows.
fn record_failing(argv: &[String]) -> crate::g2::open_with_properties::lane::Exit {
    LAUNCHED.lock().unwrap().push(argv.to_vec());
    (Some(1), String::new())
}

// SW-03: Open in Terminal and Open as Text launch the row's argv and say v0.3.12's failure sentence.
#[gpui::test]
fn sw03_terminal_and_text_rows_launch_their_argv(cx: &mut TestAppContext) {
    for (action, flag, failed) in [
        (
            "openTerminal",
            "--terminal",
            "No terminal on this system opened that folder.",
        ),
        (
            "openAsText",
            "--open-text",
            "No text editor opened that file.",
        ),
    ] {
        LAUNCHED.lock().unwrap().clear();
        let mut fixture = Fixture::new(cx, 0o644);
        update(cx, fixture.handle, |shell, _, _| {
            shell.launch.settings.data =
                jsondoc::parse(r#"{"menu":{"hidden":["openAgent","actions","runScript"]}}"#)
                    .unwrap();
            shell.opener[0].launch = record_failing;
        });
        fixture.open(cx, 1, vec![]);
        update(cx, fixture.handle, |shell, window, cx| {
            shell.menu_outcome(
                crate::g2::context_menus::menu::Outcome::Fire {
                    action: action.into(),
                    leaf: String::new(),
                },
                window,
                cx,
            )
        });
        let validate = fixture
            .drain()
            .into_iter()
            .find(|r| r.get("op").and_then(Json::as_str) == Some("validate"))
            .expect("the row validates its snapshot");
        let file = fixture
            .root
            .join("notes.txt")
            .to_string_lossy()
            .into_owned();
        fixture.receive(cx, &format!("{{\"t\":\"menuaction\",\"id\":{},\"op\":\"validate\",\"ok\":true,\"action\":\"{action}\",\"paths\":[\"{}\"]}}", number(&validate), flea::json::escape(&file)));
        cx.run_until_parked();
        // #373: the row itself reaches flea --terminal, which opens a file row's own folder.
        let launched = LAUNCHED.lock().unwrap().clone();
        assert_eq!(launched.len(), 1, "{action}: {launched:?}");
        assert_eq!(launched[0][1..], [flag.to_owned(), file], "{action}");
        assert_eq!(
            read(cx, fixture.handle, "lastMessage").stdout.trim(),
            failed,
            "{action}"
        );
    }
}
