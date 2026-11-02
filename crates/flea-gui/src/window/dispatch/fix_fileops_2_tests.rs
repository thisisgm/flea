//! EY-524: a read-only folder has no Trash, so every trash key refuses before acting, as v0.3.12's trashRefused does.
use super::{
    fileops_tests::wire,
    test_shell::{land, open_root_with, row},
};
use crate::window::navigate::Step;
use gpui::{Entity, TestAppContext, VisualTestContext};

const FOLDER: &str = "/work/A";

// The scene's folder landed with alpha.txt under the cursor, its listed "w" pinned; the wire reader comes back.
fn landed<'a>(
    cx: &'a mut TestAppContext,
    preset: &str,
    writable: bool,
) -> (
    Entity<crate::state::Shell>,
    &'a mut VisualTestContext,
    impl FnMut() -> Vec<String>,
) {
    let (shell, cx) = open_root_with(cx, &format!(r#""keys":"{preset}","view":"list""#), None);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let _ = shell.step(0, Step::OpenPlace(FOLDER.into()), window, cx);
        })
    });
    land(&shell, cx, FOLDER, vec![row("alpha.txt", false)], 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| window.focus(&shell.focus, cx));
    });
    let sent = wire(&shell, cx);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        host.backend
            .as_ref()
            .unwrap()
            .test_writable(FOLDER, writable);
    });
    (shell, cx, sent)
}

#[gpui::test]
fn fix_fileops_2_read_only_folder_refuses_every_trash_key(cx: &mut TestAppContext) {
    for (preset, keys) in [
        ("default", "delete"),
        ("default", "d"),
        ("default", "d d"),
        ("mac", "ctrl-delete"),
        ("vim", "shift-d"),
    ] {
        let (shell, cx, mut sent) = landed(cx, preset, false);
        cx.simulate_keystrokes(keys);
        assert!(
            sent().is_empty(),
            "{preset} {keys}: nothing is trashed or reviewed"
        );
        let (primary, secondary) = shell.update(cx, |shell, cx| {
            let catalog = shell.locale.catalog.clone();
            shell.status_messages(cx, |messages, _| {
                let line = messages.line(&catalog, "");
                (line.primary, line.secondary)
            })
        });
        assert_eq!(primary, "This location has no Trash", "{preset} {keys}");
        assert!(
            secondary.ends_with(" deletes"),
            "{preset} {keys}: {secondary}"
        );
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn fix_fileops_2_writable_folder_still_trashes(cx: &mut TestAppContext) {
    let (shell, cx, mut sent) = landed(cx, "default", true);
    cx.simulate_keystrokes("delete");
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[0],"menuId":0,"listing":1}"#]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn fix_fileops_2_a_refused_d_ends_the_armed_pair(cx: &mut TestAppContext) {
    let (shell, cx, mut sent) = landed(cx, "default", true);
    let pin = |shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext, writable| {
        shell.update(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            host.backend
                .as_ref()
                .unwrap()
                .test_writable(FOLDER, writable);
        })
    };
    cx.simulate_keystrokes("d");
    pin(&shell, cx, false);
    cx.simulate_keystrokes("d");
    pin(&shell, cx, true);
    cx.simulate_keystrokes("d");
    assert!(
        sent().is_empty(),
        "a d after a refused d arms again instead of trashing"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
