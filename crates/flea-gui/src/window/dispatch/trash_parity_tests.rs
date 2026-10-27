//! Mounted listing regressions for Trash landing, refusal and Escape ownership.
use super::{
    fileops_tests::{reply, wire},
    test_shell::{land, lists, open_root_with, row, set_cursor},
};
use crate::window::navigate::Step;
use gpui::TestAppContext;

const SMB: &str = "/run/user/1000/gvfs/smb-share:server=host,share=docs";

#[gpui::test]
#[ignore = "known red until the path work selects the replacement row after a trash"]
fn trash_parity_replacement_row_is_selected_in_every_preset(cx: &mut TestAppContext) {
    for (preset, key) in [
        ("mac", "ctrl-delete"),
        ("vim", "shift-d"),
        ("windows", "ctrl-d"),
    ] {
        let (shell, cx) = open_root_with(cx, &format!(r#""keys":"{preset}","view":"list""#), None);
        let mut rows = vec![
            row("alpha", true),
            row("notes.txt", false),
            row("todo.md", false),
        ];
        rows[2].size = 48;
        rows.extend((0..7).map(|n| row(&format!("z{n}"), false)));
        land(&shell, cx, "/files", rows.clone(), 1);
        set_cursor(&shell, cx, 1);
        let mut sent = wire(&shell, cx);
        cx.simulate_keystrokes(key);
        assert_eq!(
            sent(),
            [r#"{"c":"trash","rows":[1],"menuId":0,"listing":1}"#]
        );
        let before = lists(&shell, cx);
        reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
        assert_eq!(
            lists(&shell, cx),
            before + 1,
            "Trash re-reads the directory"
        );
        rows.remove(1);
        land(&shell, cx, "/files", rows, 2);
        let (cursor, selected, counts) = shell.update(cx, |shell, cx| {
            shell.sync_footer(1100., cx);
            let host = shell.panes[0].as_ref().unwrap();
            (
                host.pane.cursor,
                host.selected.clone(),
                shell.surfaces[0]
                    .footer
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .counts
                    .to_string(),
            )
        });
        assert_eq!((cursor, selected), (1, vec![1]), "{preset}");
        assert_eq!(counts, "1 of 9 selected · 48 B", "{preset}");
        cx.simulate_keystrokes("down");
        assert_eq!(
            shell.read_with(cx, |shell, _| shell.panes[0]
                .as_ref()
                .unwrap()
                .selected
                .clone()),
            vec![2],
            "the lone selection follows the cursor"
        );
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn trash_parity_escape_cancels_trash_and_vim_pairs_before_escape_up(cx: &mut TestAppContext) {
    for arm in ["d", "y", "g"] {
        let (shell, cx) = open_root_with(cx, r#""keys":"vim","view":"list","escapeUp":true"#, None);
        land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
        let before = lists(&shell, cx);
        cx.simulate_keystrokes(arm);
        cx.simulate_keystrokes("escape");
        assert_eq!(
            lists(&shell, cx),
            before,
            "Escape cancels {arm} before climbing"
        );
        cx.simulate_keystrokes("escape");
        assert_eq!(lists(&shell, cx), before + 1, "the next Escape climbs");
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn trash_parity_gvfs_keys_refuse_with_the_working_delete_hint(cx: &mut TestAppContext) {
    for (preset, key) in [
        ("mac", "ctrl-delete"),
        ("vim", "shift-d"),
        ("windows", "ctrl-d"),
    ] {
        let (shell, cx) = open_root_with(cx, &format!(r#""keys":"{preset}","view":"list""#), None);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let _ = shell.step(0, Step::OpenPlace(SMB.into()), window, cx);
            })
        });
        land(&shell, cx, SMB, vec![row("notes.txt", false)], 1);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| window.focus(&shell.focus, cx));
        });
        let mut sent = wire(&shell, cx);
        cx.simulate_keystrokes(key);
        assert!(
            sent().is_empty(),
            "{preset}: a location without Trash never sends a write"
        );
        let text = shell.update(cx, |shell, cx| {
            shell.status_messages(cx, |messages, _| messages.shown().text)
        });
        assert_eq!(text, "This location has no Trash", "{preset}");
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}
