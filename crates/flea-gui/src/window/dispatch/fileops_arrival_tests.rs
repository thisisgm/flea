//! New Folder opens Rename on the new folder once its re-read lands, as PaneWire.qml openRenameOnArrival does.
use super::{
    fileops_tests::{creation_request, reply, wire},
    test_shell::{asked_on, land, land_on, lists, open, row, set_cursor},
};
use crate::{
    backend::Backend,
    window::{input::ActionEvent, startup::View},
};
use gpui::TestAppContext;
use std::{cell::RefCell, rc::Rc};

#[gpui::test]
fn a_new_folder_rename_returns_to_its_pane_after_focus_moves(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("alpha", true), row("notes.txt", false)],
        1,
    );
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Dual, window, cx)));
    let right = asked_on(&shell, cx, 1);
    land_on(
        &shell,
        cx,
        1,
        &right,
        vec![row("other.txt", false), row("zebra.txt", false)],
        2,
        1,
    );
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("ctrl-shift-n");
    assert_eq!(
        sent(),
        [creation_request(
            &shell,
            cx,
            r#"{"c":"mkdir","path":"/files","defaultBase":"New Folder"}"#
        )]
    );
    cx.simulate_keystrokes("tab down");
    assert_eq!(shell.read_with(cx, |shell, _| shell.focus_side), 1);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[1].as_ref().unwrap().pane.cursor),
        1
    );
    let before = lists(&shell, cx);
    reply(
        &shell,
        cx,
        r#"{"t":"made","ok":true,"path":"/files/New Folder"}"#,
    );
    assert_eq!(lists(&shell, cx), before + 1, "the folder is read again");
    assert!(
        shell.read_with(cx, |shell, _| shell.rename.edit.is_none()),
        "nothing is renamed before the new row lands"
    );
    let rows = vec![
        row("New Folder", true),
        row("alpha", true),
        row("notes.txt", false),
    ];
    land(&shell, cx, "/files", rows.clone(), 2);
    let cursor = shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor);
    assert_eq!(cursor, 0, "the new folder holds the cursor");
    assert!(
        shell.read_with(cx, |shell, _| shell.operations.fileops.arrival[0].is_none()),
        "the arrival is consumed by the editor it opens"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| {
            let edit = shell.rename.edit.as_ref().expect("Rename opens on arrival");
            assert_eq!(
                shell.dual_focus.last_side, 0,
                "Rename restores the owning dual pane"
            );
            assert_eq!(shell.focus_side, 0);
            (
                edit.side,
                shell.renaming_row(0),
                edit.name.clone(),
                edit.source.clone(),
            )
        }),
        (
            0,
            Some(0),
            "New Folder".to_owned(),
            "/files/New Folder".to_owned()
        ),
        "Rename is routed to the folder's pane"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[1].as_ref().unwrap().pane.cursor),
        1
    );
    // One arrival opens one editor: a later landing of the same folder asks nothing more.
    cx.simulate_keystrokes("escape");
    assert!(shell.read_with(cx, |shell, _| shell.rename.edit.is_none()));
    land(&shell, cx, "/files", rows, 3);
    assert!(shell.read_with(cx, |shell, _| shell.rename.edit.is_none()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_hidden_right_pane_finishes_new_folder_without_renaming_the_left_pane(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("alpha", true), row("notes.txt", false)],
        1,
    );
    set_cursor(&shell, cx, 1);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Dual, window, cx)));
    let right = asked_on(&shell, cx, 1);
    land_on(&shell, cx, 1, &right, vec![row("other.txt", false)], 1, 1);
    let (backend, mut requests) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[1].as_mut().unwrap().backend = Some(backend)
    });
    let renamed = Rc::new(RefCell::new(Vec::new()));
    let sink = renamed.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |shell, event: &ActionEvent, cx| {
            if event.action == "rename" {
                sink.borrow_mut().push(shell.read(cx).focus_side);
            }
        })
    });
    cx.simulate_keystrokes("tab");
    assert_eq!(shell.read_with(cx, |shell, _| shell.focus_side), 1);
    cx.simulate_keystrokes("ctrl-shift-n");
    let mkdir: Vec<_> = std::iter::from_fn(&mut requests)
        .filter(|line| flea::json::field_str(line, "c").as_deref() == Some("mkdir"))
        .collect();
    assert_eq!(
        mkdir,
        [creation_request(
            &shell,
            cx,
            &format!(
                r#"{{"c":"mkdir","path":"{}","defaultBase":"New Folder"}}"#,
                flea::json::escape(&right)
            )
        )]
    );
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::List, window, cx)));
    assert_eq!(shell.read_with(cx, |shell, _| shell.view), View::List);
    assert!(shell.read_with(cx, |shell, _| shell.panes[1].is_some()));
    let before = shell.read_with(cx, |shell, _| {
        shell.panes[1].as_ref().unwrap().nav.counters.lists
    });
    let left_before = lists(&shell, cx);
    let path = format!("{}/New Folder", right.trim_end_matches('/'));
    let made = format!(
        r#"{{"t":"made","ok":true,"path":"{}"}}"#,
        flea::json::escape(&path)
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(1, Backend::test_reply_line(&made), window, cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[1]
            .as_ref()
            .unwrap()
            .nav
            .counters
            .lists),
        before + 1,
        "the hidden owning pane still re-reads the made folder"
    );
    assert_eq!(lists(&shell, cx), left_before);
    land_on(
        &shell,
        cx,
        1,
        &right,
        vec![row("New Folder", true), row("other.txt", false)],
        2,
        2,
    );
    assert_eq!(
        *renamed.borrow(),
        Vec::<usize>::new(),
        "a hidden pane opens no Rename"
    );
    assert!(shell.read_with(cx, |shell, _| shell.operations.fileops.arrival[1].is_none()));
    assert_eq!(shell.read_with(cx, |shell, _| shell.focus_side), 0);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
        1
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_duplicate_lands_on_its_copy_without_rename(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    let _sent = wire(&shell, cx);
    let asked = Rc::new(RefCell::new(Vec::new()));
    let sink = asked.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, event: &ActionEvent, _| {
            sink.borrow_mut().push(event.action)
        })
    });
    reply(
        &shell,
        cx,
        r#"{"t":"duplicated","ok":true,"path":"/files/notes copy.txt"}"#,
    );
    land(
        &shell,
        cx,
        "/files",
        vec![row("notes copy.txt", false), row("notes.txt", false)],
        2,
    );
    assert!(
        asked.borrow().is_empty(),
        "PaneWire.qml onDuplicated arms no editor"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
