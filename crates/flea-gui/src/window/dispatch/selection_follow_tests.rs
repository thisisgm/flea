//! EY-166: the first move after creating and naming a folder keeps the selection on the cursor.
use super::super::{
    fileops_tests::{reply, wire},
    key_rows_tests::state,
    menus_tests::settle,
    rename_tests::editing,
    test_shell::{land, open_root_with, row},
};
use crate::{state::Shell, window::u07_tests::read};
use flea::protocol::RowFacts;
use gpui::{Entity, TestAppContext, VisualTestContext};

fn sized(name: &str, dir: bool, size: u64) -> RowFacts {
    RowFacts {
        size,
        ..row(name, dir)
    }
}

fn footer(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    let value = flea::jsondoc::parse(&read(shell, cx, "statusFooterState", &[])).unwrap();
    value
        .get("left")
        .unwrap()
        .get("text")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

fn home(folder: Option<&str>) -> Vec<RowFacts> {
    let mut rows: Vec<_> = [
        "A", "Code", "Docs", "Empty", "Kinds", "Locked", "Many", "Media",
    ]
    .map(|name| sized(name, true, 0))
    .into();
    if let Some(folder) = folder {
        rows.push(sized(folder, true, 0));
    }
    rows.extend([sized("notes.txt", false, 99), sized("todo.md", false, 48)]);
    rows
}

#[gpui::test]
fn ey166_down_after_new_folder_moves_selection_and_footer(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(
        cx,
        r#""keys":"default","view":"list""#,
        Some("/files/notes.txt"),
    );
    land(&shell, cx, "/files", home(None), 1);
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("ctrl-shift-n");
    assert_eq!(
        sent(),
        [r#"{"c":"mkdir","path":"/files","defaultBase":"New Folder","request":1}"#]
    );
    reply(
        &shell,
        cx,
        r#"{"t":"made","ok":true,"path":"/files/New Folder"}"#,
    );
    land(&shell, cx, "/files", home(Some("New Folder")), 2);
    assert_eq!(editing(&shell, cx).unwrap().1, "New Folder");
    cx.simulate_input("SA6 today");
    cx.simulate_keystrokes("enter");
    reply(
        &shell,
        cx,
        r#"{"t":"renamed","ok":true,"path":"/files/SA6 today"}"#,
    );
    land(&shell, cx, "/files", home(Some("SA6 today")), 3);
    assert!(editing(&shell, cx).is_none());
    assert_eq!(state(&shell, cx), (8, vec![8]));
    let before = footer(&shell, cx);
    assert!(before.starts_with("1 of 11 selected"), "{before}");
    cx.simulate_keystrokes("down");
    settle(cx);
    let actual = (state(&shell, cx), footer(&shell, cx));
    assert_eq!(
        actual.0,
        (9, vec![9]),
        "cursor, selection and footer: {actual:?}"
    );
    assert!(actual.1.starts_with("1 of 11 selected"), "{actual:?}");
    assert!(actual.1.contains("99 B"), "{actual:?}");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
