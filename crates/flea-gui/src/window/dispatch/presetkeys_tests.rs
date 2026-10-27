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
            let lines = sent();
            assert!(
                lines
                    .iter()
                    .any(|line| line.contains(r#""op":"properties""#)),
                "{preset} {} asks the facts of notes.txt: {lines:?}",
                binding.keys
            );
            assert!(
                shell.read_with(cx, |shell, _| shell.properties.is_some()),
                "{preset} {} opens the Properties card",
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
