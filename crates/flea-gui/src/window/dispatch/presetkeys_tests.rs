//! EY-226: the preset rows that open Properties and the mac paste chords reach their handlers from the production key path.
use super::{
    menus_tests::home,
    test_shell::{keystroke, land, open, sent, set_cursor},
};
use crate::{backend::Backend, state::Shell};
use flea::keys::{self, Frontend};
use gpui::{Entity, TestAppContext, VisualTestContext};

// A backend reply line, parsed by the production transport and handed to the Shell as the reader does.
fn reply(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    let event = Backend::test_reply_line(line);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.receive(0, event, window, cx)));
    cx.run_until_parked();
}

// The copied file the clipboard watch reports, as key_rows_tests delivers it.
fn copied(shell: &Entity<Shell>, cx: &mut VisualTestContext, path: &str) {
    let line =
        format!(r#"{{"t":"clip","op":"changed","clip":"copy","paths":["{path}"],"token":"T1"}}"#);
    reply(shell, cx, &line);
}

#[gpui::test]
fn every_properties_row_in_the_windows_and_mac_presets_opens_the_card(cx: &mut TestAppContext) {
    for preset in ["windows", "mac"] {
        for binding in
            keys::binding_rows(preset, Frontend::Gui).filter(|row| row.action == "properties")
        {
            // One fresh window per press, so an open card from one row cannot answer the next.
            let (shell, cx) = open(cx, preset);
            land(&shell, cx, "/files", home(), 1);
            set_cursor(&shell, cx, 2);
            sent();
            cx.simulate_event(keystroke(binding));
            cx.run_until_parked();
            // EY-226: the snapshot names its listing and goes alone; Properties waits for its answer.
            let lines = sent();
            let snapshot = lines
                .iter()
                .find(|line| line.contains(r#""op":"snapshot""#))
                .unwrap_or_else(|| {
                    panic!("{preset} {} snapshots notes.txt: {lines:?}", binding.keys)
                });
            let id = flea::json::field_usize(snapshot, "id").unwrap();
            assert!(
                snapshot.contains(r#""listing":1,"rows":[2],"cursor":2"#),
                "{preset} {} names the listing its row came from: {snapshot}",
                binding.keys
            );
            assert!(
                !lines
                    .iter()
                    .any(|line| line.contains(r#""op":"properties""#)),
                "{preset} {} waits for the snapshot before asking: {lines:?}",
                binding.keys
            );
            reply(
                &shell,
                cx,
                &format!(
                    r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[33188]}}"#
                ),
            );
            let lines = sent();
            assert_eq!(
                lines,
                [format!(
                    r#"{{"c":"menuaction","op":"properties","id":{id}}}"#
                )],
                "{preset} {} asks the facts of notes.txt once the snapshot is held",
                binding.keys
            );
            reply(
                &shell,
                cx,
                &format!(
                    r#"{{"t":"menuaction","id":{id},"op":"properties","ok":true,"path":"/files/notes.txt","kind":"File","bytes":99,"modified":0,"mode":"0644","owner":"gm","uid":1000,"gid":1000}}"#
                ),
            );
            assert!(
                shell.read_with(cx, |shell, _| shell
                    .properties
                    .as_ref()
                    .is_some_and(|open| open.ready())),
                "{preset} {} shows the facts on the Properties card",
                binding.keys
            );
            assert_eq!(
                shell.read_with(cx, |shell, _| {
                    let host = shell.panes[0].as_ref().unwrap();
                    (
                        host.pane.cursor,
                        host.pane.held.row(2).map(|row| row.name.to_string()),
                    )
                }),
                (2, Some("notes.txt".into())),
                "{preset} {} keeps the listing behind the card",
                binding.keys
            );
            shell.update(cx, |shell, _| shell.stop_for_app_quit());
        }
    }
}

#[gpui::test]
fn mac_super_v_pastes_the_copied_file_into_the_open_folder(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "mac");
    land(&shell, cx, "/files", home(), 1);
    copied(&shell, cx, "/other/todo.md");
    sent();
    let binding = keys::binding_rows("mac", Frontend::Gui)
        .find(|row| row.keys == "super-v")
        .expect("mac binds super-v to paste");
    cx.simulate_event(keystroke(binding));
    cx.run_until_parked();
    let lines = sent();
    assert!(
        lines.iter().any(|line| line.contains(r#""dest":"/files""#) && line.contains(r#""/other/todo.md""#)),
        "super-v asks to paste the copied file into /files: {lines:?}"
    );
    reply(
        &shell,
        cx,
        r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
    );
    let lines = sent();
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""c":"transfer""#) && line.contains(r#""op":"copy""#)),
        "super-v copies it: {lines:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn mac_super_alt_v_moves_the_copied_file_into_the_open_folder(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "mac");
    land(&shell, cx, "/files", home(), 1);
    copied(&shell, cx, "/other/todo.md");
    sent();
    let binding = keys::binding_rows("mac", Frontend::Gui)
        .find(|row| row.keys == "superalt-v")
        .expect("mac binds superalt-v to move here");
    cx.simulate_event(keystroke(binding));
    cx.run_until_parked();
    let lines = sent();
    assert!(
        lines.iter().any(|line| line.contains(r#""dest":"/files""#) && line.contains(r#""/other/todo.md""#)),
        "superalt-v asks to move the copied file into /files: {lines:?}"
    );
    reply(
        &shell,
        cx,
        r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
    );
    let lines = sent();
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""c":"transfer""#) && line.contains(r#""op":"move""#)),
        "superalt-v moves it: {lines:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
