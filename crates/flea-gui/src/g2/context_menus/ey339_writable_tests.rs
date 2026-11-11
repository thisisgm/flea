//! EY-339: the background menu greys every writing row where the backend's listed line says the folder is unwritable.
use crate::backend::{Backend, Event};
use flea::ui_logic_context_menus::build::listing_entries;
use gpui::TestAppContext;
use std::process::Command;

// One listed line as the backend writes it, with the folder's "w" pinned rather than read from a disk.
fn listed(path: &str, writable: bool) -> String {
    format!(
        r#"{{"t":"listed","n":0,"read":0.000,"sort":0.000,"v":1,"w":{writable},"path":"{path}"}}"#
    )
}

// The child stands in for the backend: it answers the first List with the given listed lines, in order.
fn disabled_rows(lines: &[String], cx: &mut TestAppContext) -> (bool, Vec<String>) {
    let mut command = Command::new("/bin/bash");
    command.args([
        "-c",
        r#"IFS= read -r line
printf '%s\n' "$@"
while IFS= read -r line; do :; done
"#,
        "listed",
    ]);
    command.args(lines);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    let handle = cx.add_window(|_, cx| crate::state::tests::shell_with(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            let replies = host.backend.as_ref().unwrap().replies.clone();
            let event = replies.recv_blocking().unwrap();
            assert!(
                matches!(event, Event::Reply(_)),
                "the listed reply: {event:?}"
            );
            // Every later line is read before the menu is built, so its folder fact has landed.
            for _ in 1..lines.len() {
                let later = replies.recv_blocking().unwrap();
                assert!(
                    matches!(later, Event::Reply(_)),
                    "a later listed reply: {later:?}"
                );
            }
            shell.receive(0, event, window, cx);
            let context = shell.listing_menu_context(0, &[], 0, true);
            let disabled = listing_entries(&context)
                .into_iter()
                .filter(|entry| entry.disabled)
                .map(|entry| entry.action.to_string())
                .collect();
            shell.stop_for_app_quit();
            (context.dir_writable, disabled)
        })
        .unwrap()
}

fn assert_greyed(disabled: &[String]) {
    for action in ["newFolder", "newFile"] {
        assert!(
            disabled.iter().any(|id| id == action),
            "Menu.js greys {action} in a folder the user cannot write: {disabled:?}"
        );
    }
}

#[gpui::test]
fn ey339_an_unwritable_folder_greys_new_folder_and_new_file(cx: &mut TestAppContext) {
    let (writable, disabled) = disabled_rows(&[listed("/files", false)], cx);
    assert!(
        !writable,
        "the listed line said the folder cannot be written"
    );
    assert_greyed(&disabled);
}

#[gpui::test]
fn ey339_a_writable_folder_keeps_new_folder_and_new_file(cx: &mut TestAppContext) {
    let (writable, disabled) = disabled_rows(&[listed("/files", true)], cx);
    assert!(writable);
    for action in ["newFolder", "newFile"] {
        assert!(
            !disabled.iter().any(|id| id == action),
            "{action} stays live: {disabled:?}"
        );
    }
}

#[gpui::test]
fn ey339_a_listing_of_another_folder_keeps_this_folder_unwritable(cx: &mut TestAppContext) {
    let lines = [listed("/files", false), listed("/elsewhere", true)];
    let (writable, disabled) = disabled_rows(&lines, cx);
    assert!(
        !writable,
        "a later listed line for another folder says nothing about /files"
    );
    assert_greyed(&disabled);
}

#[test]
fn ey339_each_folder_keeps_its_own_listed_fact() {
    let mut folders = crate::backend::Writable::default();
    folders.record("/ro".into(), false);
    folders.record("/rw".into(), true);
    assert!(!folders.of("/ro"));
    assert!(folders.of("/rw"));
    assert!(
        folders.of("/unlisted"),
        "a folder no listed line named reads writable"
    );
    folders.record("/ro".into(), true);
    assert!(
        folders.of("/ro"),
        "a re-list that can write again lifts the grey"
    );
    folders.record("/ro".into(), false);
    for n in 0..16 {
        folders.record(format!("/other-{n}"), true);
    }
    assert!(folders.of("/ro"), "only the last sixteen folders are kept");
}
