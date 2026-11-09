use super::*;
use crate::window::startup::View;

pub(super) fn check() -> Vec<String> {
    let mut failed = Vec::new();
    for view in [View::List, View::Columns, View::Grid, View::Dual] {
        case(
            &format!("main-{view:?}"),
            &mut failed,
            |shell, window, cx| {
                if view == View::Dual {
                    cx.set_global(crate::state::constructor_fixture::Services {
                        backend: |_, cx| Ok(channel(cx)),
                        monitor: None,
                    });
                    shell.launch.paths[1] = "/right".into();
                    shell.apply_view(View::Dual, window, cx);
                    let right = shell.panes[1].as_mut().expect("Dual starts the right pane");
                    right
                        .pane
                        .receive(flea::protocol::Reply::Listed {
                            path: "/right".into(),
                            total: 1,
                        })
                        .unwrap();
                    right
                        .pane
                        .receive(flea::protocol::Reply::Rows {
                            start: 0,
                            listing: 3,
                            rows: vec![flea::protocol::RowFacts {
                                name: "right-font-file.txt".into(),
                                mode: 0o100644,
                                ..Default::default()
                            }],
                        })
                        .unwrap();
                    right.pane.commit(0);
                    right.changed(&shell.locale);
                    shell.settle_dual(window, cx);
                    assert_eq!(shell.view, View::Dual, "the right pane settled Dual");
                } else {
                    shell.view = view;
                }
                shell.mount_rail(cx);
                shell
                    .rail
                    .as_ref()
                    .unwrap()
                    .places
                    .update(cx, |producer, cx| {
                        producer.fixture_home(&std::env::var("HOME").unwrap(), "", cx);
                    });
            },
        );
    }
    case("status-message", &mut failed, |shell, _, cx| {
        shell.status_note("Font gate footer", cx);
    });
    case("jobs", &mut failed, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx);
        shell.overlay.as_ref().unwrap().update(cx, |view, cx| {
            view.model.dialog = None;
            view.model
                .jobs
                .snapshot(vec![flea::ui_logic_file_ops::JobRow {
                    id: 1,
                    kind: "copy".into(),
                    state: "running".into(),
                    file: "Font gate copying.txt".into(),
                    head: "Font gate job".into(),
                    total: 2,
                    total_bytes: Some(128),
                    bytes: Some(64),
                    ..Default::default()
                }]);
            cx.notify();
        });
    });
    // The Plugins page's own rows are gated in settings.rs; its section label is a nav row here.
    for section in flea::ui_logic_settings_pages::pages::SECTIONS
        .iter()
        .filter(|section| section.id != "plugins")
    {
        case(
            &format!("settings-{}", section.id),
            &mut failed,
            |shell, window, cx| {
                shell.open_settings(window, cx);
                shell
                    .settings
                    .panel
                    .as_ref()
                    .unwrap()
                    .update(cx, |view, cx| {
                        view.panel.show_section(section.id);
                        cx.notify();
                    });
            },
        );
    }
    case("settings-search", &mut failed, |shell, window, cx| {
        shell.open_settings(window, cx);
        shell
            .settings
            .panel
            .as_ref()
            .unwrap()
            .update(cx, |view, cx| {
                view.panel.query = Some("font".into());
                cx.notify();
            });
    });
    case("keyboard-sheet", &mut failed, |shell, window, cx| {
        shell.open_sheet(window, cx)
    });
    case("copy-to-dialog", &mut failed, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx)
    });
    case("move-to-dialog", &mut failed, |shell, window, cx| {
        shell.open_overlay_copy_to(true, "/files", window, cx)
    });
    case("trash", &mut failed, |shell, window, cx| {
        shell.mount_trash(window, cx);
        shell
            .trash
            .as_ref()
            .unwrap()
            .pane
            .update(cx, |pane, _| pane.monitor_start = None);
        assert!(shell.trash_action("openTrash", window, cx));
        for (op, fields) in [
            (
                "list",
                r#", "total":1,"start":0,"rows":[{"uri":"trash:///font-gate-row","original":"/files/Font gate trashed.txt","size":64,"identity":"font-gate"}]"#,
            ),
            ("summary", r#", "bytes":64,"partial":false"#),
        ] {
            let host = shell.trash.as_ref().unwrap();
            let id = host.pane.read(cx).view().request_id;
            let side = host.side;
            shell.receive(
                side,
                crate::backend::Event::TrashLine(format!(
                    r#"{{"t":"trashbrowse","id":{id},"op":"{op}","ok":true{fields}}}"#
                )),
                window,
                cx,
            );
        }
        shell.sync_trash_presentation(window, cx);
    });
    failed
}

pub(super) fn case(
    name: &str,
    failed: &mut Vec<String>,
    setup: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>),
) {
    case_with(name, &[], failed, setup)
}

pub(super) fn case_with(
    name: &str,
    extra: &[&str],
    failed: &mut Vec<String>,
    setup: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>),
) {
    let (mut cx, recorder) = context();
    recorder.begin();
    let handle = open_with(&mut cx, setup);
    draw(&mut cx, handle);
    if name == "jobs" {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(600));
        draw(&mut cx, handle);
    }
    let mut required = match name {
        "status-message" => vec!["Font gate footer"],
        "jobs" => vec!["Font gate copying.txt"],
        "settings-view" => vec!["Row density"],
        "settings-places" => vec!["Sidebar width"],
        "settings-preview" => vec!["Thumbnail size"],
        "settings-keys" => vec!["Keybinding preset"],
        "settings-display" => vec!["Text size"],
        "settings-menus" => vec!["Show keyboard hints"],
        "settings-about" => vec!["A file manager for Omarchy"],
        "settings-search" => vec!["Settings", "font"],
        "keyboard-sheet" => vec!["Keyboard Shortcuts"],
        "copy-to-dialog" => vec!["Copy To", "DESTINATION FOLDER"],
        "move-to-dialog" => vec!["Move To", "DESTINATION FOLDER"],
        "bulk-rename" => vec!["Rename 2 Items"],
        "unlock" => vec!["Unlock Font gate encrypted drive"],
        "properties" => vec!["Properties", "Close"],
        "run-file" => vec!["run.sh is a program.", "Run in Terminal"],
        "confirm-delete" => vec!["Delete 2 items permanently?"],
        "confirm-empty-trash" => vec!["Empty Trash"],
        "new-file" => vec!["New file"],
        "fat-name-confirm" => vec!["Rename incompatible names"],
        "collision-confirm" => vec!["Replace existing files"],
        "compress-password" => vec!["Compress 1 Item"],
        "extract-password" => vec!["archive.7z is encrypted."],
        "flash" => vec!["Flash image"],
        "open-with" => vec!["With"],
        "permissions" => vec!["Permissions"],
        "output-Print" => vec!["Print"],
        "output-Convert" => vec!["Convert"],
        "output-Combine" => vec!["Combine"],
        "add-network-share" => vec!["Add a Network Share"],
        "settings-plugin-font.gate" => vec!["Font gate choice"],
        "settings-plugin-add" => vec!["Add a Plugin"],
        "settings-plugin-remove" => vec!["Remove Font gate plugin?"],
        "context-menu-background" => vec!["Sort By"],
        _ => Vec::new(),
    };
    required.extend(extra);
    if name.starts_with("main-") {
        required.extend(["file-0.txt", "Home", "8 items"]);
    }
    if name == "main-Dual" {
        required.push("right-font-file.txt");
    }
    if name == "find" {
        required.push("Font gate query");
    }
    if name == "explore-filter" {
        required.push("esc clears");
    }
    if name == "trash" {
        required.extend(["Empty Trash", "Font gate trashed.txt"]);
    }
    if name == "context-menu-and-submenu" {
        required.extend(["Font menu", "Font submenu"]);
    }
    if !assert_runs(name, &recorder, &required) {
        failed.push(name.into());
    }
}
