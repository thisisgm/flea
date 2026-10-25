//! New Folder opens Rename on the new folder once its re-read lands, as PaneWire.qml openRenameOnArrival does.
use super::{
    fileops_tests::{reply, wire},
    test_shell::{land, lists, open, row},
};
use crate::window::input::ActionEvent;
use gpui::TestAppContext;
use std::{cell::RefCell, rc::Rc};

#[gpui::test]
fn a_new_folder_takes_the_cursor_and_runs_rename_once_its_row_lands(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", vec![row("alpha", true), row("notes.txt", false)], 1);
    let mut sent = wire(&shell, cx);
    let asked = Rc::new(RefCell::new(Vec::new()));
    let sink = asked.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, event: &ActionEvent, _| sink.borrow_mut().push(event.action))
    });
    cx.simulate_keystrokes("ctrl-shift-n");
    assert_eq!(sent(), [r#"{"c":"mkdir","path":"/files"}"#]);
    let before = lists(&shell, cx);
    reply(&shell, cx, r#"{"t":"made","ok":true,"path":"/files/New Folder"}"#);
    assert_eq!(lists(&shell, cx), before + 1, "the folder is read again");
    assert!(asked.borrow().is_empty(), "nothing is renamed before the new row lands");
    let rows = vec![row("New Folder", true), row("alpha", true), row("notes.txt", false)];
    land(&shell, cx, "/files", rows.clone(), 2);
    let cursor = shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor);
    assert_eq!(cursor, 0, "the new folder holds the cursor");
    assert_eq!(*asked.borrow(), ["rename"], "Rename runs on the new folder");
    // One arrival opens one editor: a later landing of the same folder asks nothing more.
    land(&shell, cx, "/files", rows, 3);
    assert_eq!(*asked.borrow(), ["rename"]);
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
        cx.subscribe(&shell, move |_, event: &ActionEvent, _| sink.borrow_mut().push(event.action))
    });
    reply(&shell, cx, r#"{"t":"duplicated","ok":true,"path":"/files/notes copy.txt"}"#);
    land(&shell, cx, "/files", vec![row("notes copy.txt", false), row("notes.txt", false)], 2);
    assert!(asked.borrow().is_empty(), "PaneWire.qml onDuplicated arms no editor");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
