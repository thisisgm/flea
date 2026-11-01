use super::{Key, Map};
use crate::tui::{
    action_key_tests::{drain, listing},
    actions,
    model::{Model, Tab},
    wire::{self, text},
};
use std::path::PathBuf;

thread_local! {
    // One count per live checker, so a test can prove which one check_row reached.
    pub(super) static LISTING_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(super) static PANEL_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn press(m: &mut Model, keys: &[Key], map: &Map, w: &mut wire::Wire) {
    for key in keys {
        actions::key(m, key, map, w).unwrap();
    }
}

pub(super) fn listing_action(keys: &[Key], action: &str, preset: &str, map: &Map) {
    LISTING_CHECKS.with(|count| count.set(count.get() + 1));
    if matches!(
        action,
        "suspend"
            | "pasteAbsoluteLink"
            | "pasteLink"
            | "pasteHardLink"
            | "detailBirth"
            | "detailOff"
            | "copyPath"
            | "copyUri"
            | "copydirpath"
            | "copyDirUri"
            | "copyName"
            | "copyStem"
            | "findPrevious"
            | "findNext"
            | "sortBirth"
            | "sortBirthReverse"
            | "sortExtension"
            | "sortRandom"
            | "followLink"
    ) {
        return crate::tui::parity17_tests::listing_case(keys, action);
    }
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, preset);
    let mut expected_cursor = None;
    match action {
        "cursorUp" | "cursorFirst" | "pageUp" | "halfPageUp" | "cursorDown" | "cursorLast"
        | "pageDown" | "halfPageDown" => {
            // Ten rows and a four-row page tell a line, a half page, a page and an end apart.
            for i in 2..10 {
                let mut row = m.rows[&1].clone();
                row.name = format!("f{i}");
                m.rows.insert(i, row);
            }
            m.total = 10;
            m.height = 4;
            assert_eq!(m.file_capacity(), 4);
            if matches!(action, "cursorUp" | "cursorFirst" | "pageUp" | "halfPageUp") {
                m.cursor = 9;
            }
            expected_cursor = Some(match action {
                "cursorDown" => 1,
                "halfPageDown" => 2,
                "pageDown" => 4,
                "cursorLast" => 9,
                "cursorUp" => 8,
                "halfPageUp" => 7,
                "pageUp" => 5,
                _ => 0,
            });
        }
        "selectNext" | "extendDown" => expected_cursor = Some(1),
        "escape" | "invertSelection" => {
            m.selected.insert(0);
        }
        "clearSelection" => {
            m.selected.extend([0, 1]);
        }
        "clearClipboard" => {
            m.clipboard.push("/source/file".into());
            m.cut = true;
        }
        "paste" | "movePaste" => m.clipboard.push("/source/file".into()),
        "historyBack" => m.back.push(PathBuf::from("/before")),
        "historyForward" => m.forward.push(PathBuf::from("/after")),
        "rename" => {
            m.path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            m.rows.get_mut(&0).unwrap().name = "Cargo.toml".into();
            m.rows.get_mut(&0).unwrap().directory = false;
        }
        "editFile" => m.cursor = 1,
        "print" | "convert" => {
            m.cursor = 1;
            m.rows.get_mut(&1).unwrap().name = "photo.png".into();
        }
        "tabNext" | "tabPrevious" | "tabMoveLeft" | "tabMoveRight" => {
            m.tabs.push(Tab {
                path: PathBuf::from("/second"),
                cursor: 0,
                back: Vec::new(),
                forward: Vec::new(),
            });
            if matches!(action, "tabNext" | "tabPrevious") {
                m.tabs.push(Tab {
                    path: PathBuf::from("/third"),
                    cursor: 0,
                    back: Vec::new(),
                    forward: Vec::new(),
                });
            }
            if action == "tabMoveLeft" {
                m.tab = 1;
                m.path = "/second".into();
            }
        }
        _ if action
            .strip_prefix("tab")
            .is_some_and(|n| n.parse::<usize>().is_ok()) =>
        {
            for i in 1..9 {
                m.tabs.push(Tab {
                    path: PathBuf::from(format!("/tab{i}")),
                    cursor: i,
                    back: Vec::new(),
                    forward: Vec::new(),
                });
            }
            m.tab = if action == "tab1" { 1 } else { 0 };
        }
        _ => {}
    }
    let (path, cursor, tabs, detail) = (m.path.clone(), m.cursor, m.tabs.len(), m.detail.mode);
    press(&mut m, keys, map, &mut w);
    let sent = drain(&mut w);
    let command = |c: &str| {
        sent.iter()
            .find(|v| text(v, "c") == c)
            .unwrap_or_else(|| panic!("{preset} {action}: missing {c}: {sent:?}"))
    };
    assert!(m.error.is_empty(), "{preset} {action}: {}", m.error);
    if let Some(cursor) = expected_cursor {
        assert_eq!(m.cursor, cursor, "{action}");
    }
    match action {
        "cursorDown" | "cursorUp" | "cursorFirst" | "cursorLast" | "pageUp" | "pageDown"
        | "halfPageUp" | "halfPageDown" => {}
        "extendDown" => assert!(m.selected.contains(&0) && m.selected.contains(&1)),
        "selectNext" => assert_eq!(m.selected.iter().copied().collect::<Vec<_>>(), [0]),
        "toggleSelect" => assert!(m.selected.contains(&cursor)),
        "clearSelection" | "escape" => assert!(m.selected.is_empty()),
        "selectAll" => assert_eq!(m.selected.iter().copied().collect::<Vec<_>>(), [0, 1]),
        "invertSelection" => assert_eq!(m.selected.iter().copied().collect::<Vec<_>>(), [1]),
        "copy" | "cut" => {
            assert!(m.pending_clipboard);
            assert_eq!(m.cut, action == "cut");
            command("paths");
        }
        "clearClipboard" => assert!(m.clipboard.is_empty() && !m.cut),
        "paste" | "movePaste" => {
            let ask = command("collisions");
            assert!(m.prompts.question.is_none());
            m.receive(
                crate::jsondoc::parse(&format!(
                    r#"{{"t":"collisions","id":{},"total":0}}"#,
                    wire::count(ask, "id")
                ))
                .unwrap(),
                &mut w,
            )
            .unwrap();
            let landed = drain(&mut w);
            let transfer = landed
                .iter()
                .find(|v| text(v, "c") == "transfer")
                .unwrap_or_else(|| panic!("{preset} {action}: no transfer: {landed:?}"));
            assert_eq!(
                text(transfer, "op"),
                if action == "movePaste" {
                    "move"
                } else {
                    "copy"
                },
                "{preset} {action}"
            );
        }
        "trash" => {
            command("trash");
        }
        "deletePermanently" => {
            assert!(m.deletion.is_some());
            assert_eq!(text(command("menuaction"), "op"), "snapshot");
        }
        "rename" | "newFile" | "newFolder" | "filter" | "pathBar" | "search" | "searchContents" => {
            let kind = match action {
                "newFile" => "newfile",
                "newFolder" => "mkdir",
                "pathBar" => "path",
                "searchContents" => "search",
                other => other,
            };
            assert_eq!(m.editor.as_ref().unwrap().kind, kind);
            if matches!(action, "search" | "searchContents") {
                assert_eq!(m.find.mode, action == "searchContents");
            }
        }
        "open" | "parent" | "historyBack" | "historyForward" | "goHome" | "goConfig"
        | "goDownloads" | "toggleHidden" | "tabNew" | "openTab" => {
            command("list");
            let expected = match action {
                "open" | "openTab" => path.join("folder"),
                "parent" => path.parent().unwrap().to_path_buf(),
                "historyBack" => "/before".into(),
                "historyForward" => "/after".into(),
                "goHome" | "goConfig" | "goDownloads" => {
                    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
                    if action == "goConfig" {
                        home.join(".config")
                    } else if action == "goDownloads" {
                        home.join("Downloads")
                    } else {
                        home
                    }
                }
                _ => path,
            };
            assert_eq!(m.pending.as_ref(), Some(&expected), "{action}");
            if matches!(action, "tabNew" | "openTab") {
                assert_eq!(m.tabs.len(), tabs + 1);
            }
        }
        "openTrash" => {
            command("trashbrowse");
        }
        "sortName" | "sortNameReverse" | "sortDate" | "sortDateReverse" | "sortSize"
        | "sortSizeReverse" => {
            let by = if action.starts_with("sortDate") {
                "date"
            } else if action.starts_with("sortSize") {
                "size"
            } else {
                "name"
            };
            assert_eq!(text(command("sort"), "by"), by);
            assert_eq!(m.reverse, action.ends_with("Reverse"));
            if by == "name" {
                let mut files = crate::backend::listing::Listing::new();
                for name in ["file10", "file2"] {
                    files.push(name, false);
                }
                crate::backend::ordering::request(
                    &mut files,
                    std::path::Path::new("/nonexistent"),
                    &crate::backend::mime::Db::from_str(""),
                    &crate::jsondoc::render(command("sort")),
                )
                .unwrap();
                let names = [files.name(0), files.name(1)];
                assert_eq!(
                    names,
                    if m.reverse {
                        ["file10", "file2"]
                    } else {
                        ["file2", "file10"]
                    }
                );
            }
        }
        "quit" | "quitNoCwd" => {
            assert!(m.quit);
            assert_eq!(m.no_cwd, action == "quitNoCwd");
        }
        "tabClose" => assert!(m.quit),
        "tabNext" | "tabPrevious" => {
            let (tab, path) = if action == "tabNext" {
                (1, "/second")
            } else {
                (2, "/third")
            };
            assert_eq!(m.tab, tab, "{preset} {action}");
            assert_eq!(text(command("list"), "path"), path);
        }
        "tabMoveLeft" | "tabMoveRight" => {
            assert_eq!(m.tab, if action == "tabMoveLeft" { 0 } else { 1 });
            assert_eq!(m.tabs[m.tab].path, path);
            assert_eq!(m.path, path);
            assert!(sent.is_empty());
        }
        "keymapSheet" => assert!(m.sheet && m.properties.is_none()),
        "properties" => {
            assert!(m.sheet && m.properties.is_some());
            assert_eq!(text(command("menuaction"), "op"), "snapshot");
        }
        "detailNext" => assert_ne!(m.detail.mode, detail),
        "editFile" => assert_eq!(m.edit_file.as_ref(), Some(&path.join("file"))),
        "redo" | "undo" => {
            command(action);
        }
        "focusNext" | "focusPreview" => assert!(m.preview_focus),
        "print" | "convert" => {
            assert!(if action == "print" {
                m.printing.card.is_some()
            } else {
                m.converting.card.is_some()
            });
            assert_eq!(
                m.menu_action,
                if action == "print" {
                    "printSnapshot"
                } else {
                    "convertSnapshot"
                }
            );
            assert_eq!(text(command("menuaction"), "op"), "snapshot");
        }
        "reserved" => {
            assert!(sent.is_empty() && m.key_arm.is_empty() && m.deletion.is_none());
            assert_eq!(m.path, path);
        }
        _ if action
            .strip_prefix("tab")
            .is_some_and(|n| n.parse::<usize>().is_ok()) =>
        {
            assert_eq!(m.tab, action[3..].parse::<usize>().unwrap() - 1);
            command("list");
        }
        _ => panic!("no handler assertion for {action}: {keys:?}"),
    }
    assert!(m.launches.is_empty() && m.edit_file.is_none() || action == "editFile");
    wire::finish(w, reader);
}

pub(super) fn panel(keys: &[Key], action: &str, properties: bool, map: &Map) {
    PANEL_CHECKS.with(|count| count.set(count.get() + 1));
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    m.height = 10;
    m.sheet = true;
    m.sheet_top = 2;
    if properties {
        m.properties = Some(
            (0..30)
                .map(|i| (format!("Fact {i}"), "value".into()))
                .collect(),
        );
    }
    press(&mut m, keys, map, &mut w);
    match action {
        "escape" | "open" => assert!(!m.sheet && m.properties.is_none()),
        "cursorUp" => assert_eq!(m.sheet_top, 1),
        "cursorDown" => assert_eq!(m.sheet_top, 3),
        _ => panic!("unverified panel action {action}"),
    }
    assert_eq!((m.cursor, m.path), (0, PathBuf::from("/listing")));
    assert!(drain(&mut w).is_empty() && m.launches.is_empty());
    wire::finish(w, reader);
}

pub(super) fn detail_route(field: &str, map: &Map) {
    use crate::tui::detail::{self, Column};
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "yazi");
    m.cursor = 1;
    let row = m.rows.get_mut(&1).unwrap();
    row.mode = 0o100640;
    row.modified = 1_700_000_000;
    row.size = 42;
    let (steps, column) = match field {
        "size" => (4, Column::Size),
        "modified" => (1, Column::Modified),
        "mode" => (2, Column::Mode),
        "owner" => (3, Column::Owner),
        _ => panic!("unknown detail field"),
    };
    for _ in 0..steps {
        press(&mut m, &[Key::character('I', "")], map, &mut w);
    }
    assert_eq!(m.detail.mode, column);
    if field == "owner" {
        detail::prepare(&mut m, &mut w).unwrap();
        let sent = drain(&mut w);
        let request = sent
            .iter()
            .find(|value| text(value, "c") == "owner")
            .unwrap();
        assert_eq!(wire::count(request, "row"), 1);
        let reply = crate::jsondoc::parse(&format!(
            r#"{{"t":"owner","token":{},"row":1,"owner":"fixture-owner"}}"#,
            wire::count(request, "token")
        ))
        .unwrap();
        m.receive(reply, &mut w).unwrap();
    }
    let value = detail::value(&m, 1, &m.rows[&1]);
    let expected = match field {
        "size" => m.i18n.locale().format_size(42),
        "modified" => m.i18n.locale().format_date(1_700_000_000).unwrap(),
        "mode" => m.i18n.locale().localize_number("0640"),
        "owner" => "fixture-owner".into(),
        _ => unreachable!(),
    };
    assert_eq!(value, expected);
    assert!(m.error.is_empty() && m.launches.is_empty() && m.edit_file.is_none());
    assert_eq!(m.cursor, 1);
    wire::finish(w, reader);
}
