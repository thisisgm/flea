//! Mounted listing regressions for Trash landing, refusal and Escape ownership.
use super::{
    fileops_tests::{reply, wire},
    test_shell::{land, lists, open_root_with, row, set_cursor},
};
use crate::window::navigate::Step;
use gpui::TestAppContext;

pub(super) const SMB: &str = "/run/user/1000/gvfs/smb-share:server=host,share=docs";

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
fn bfx325_no_trash_keys_review_before_permanent_delete(cx: &mut TestAppContext) {
    for (preset, key) in [
        ("default", "delete"),
        ("mac", "ctrl-delete"),
        ("vim", "shift-d"),
        ("windows", "ctrl-d"),
        ("default", "d d"),
    ] {
        for confirm in [false, true] {
            let (shell, cx) =
                open_root_with(cx, &format!(r#""keys":"{preset}","view":"list""#), None);
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
            let snapshot = sent();
            assert_eq!(snapshot.len(), 1, "{preset}: {key} must request a review");
            let id = flea::json::field_usize(&snapshot[0], "id").unwrap();
            assert_eq!(
                snapshot[0],
                format!(
                    r#"{{"c":"menuaction","op":"snapshot","id":{id},"listing":1,"rows":[0],"cursor":0}}"#
                )
            );
            reply(
                &shell,
                cx,
                &format!(
                    r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[32768]}}"#
                ),
            );
            assert_eq!(
                sent(),
                [format!(
                    r#"{{"c":"menuaction","op":"prepareDelete","id":{id}}}"#
                )]
            );
            reply(
                &shell,
                cx,
                &format!(
                    r#"{{"t":"menuaction","id":{id},"op":"prepareDelete","ok":true,"token":77,"count":1,"bytes":12}}"#
                ),
            );
            assert!(shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)));
            assert!(sent().is_empty(), "review never deletes");
            cx.simulate_keystrokes(if confirm { "tab enter" } else { "enter" });
            let out = sent();
            if confirm {
                assert!(
                    out.iter().any(|line| line
                        == &format!(r#"{{"c":"menuaction","op":"delete","id":{id},"token":77}}"#)),
                    "{out:?}"
                );
            } else {
                assert!(
                    out.iter().all(|line| !line.contains(r#""op":"delete""#)),
                    "Enter defaults to Cancel: {out:?}"
                );
                assert!(!shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)));
            }
            shell.update(cx, |shell, _| shell.stop_for_app_quit());
        }
    }
}

#[gpui::test]
fn bfx325_no_trash_pair_names_review_and_a_denied_review_never_deletes(cx: &mut TestAppContext) {
    let (shell, cx) = super::test_shell::open_root_at(cx, SMB);
    land(&shell, cx, SMB, vec![row("notes.txt", false)], 1);
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("d");
    assert!(sent().is_empty(), "the first d only arms");
    let text = shell.update(cx, |shell, cx| {
        shell.status_messages(cx, |messages, _| messages.shown().text)
    });
    assert!(text.contains("review permanent deletion"), "{text}");
    cx.simulate_keystrokes("d");
    let snapshot = sent();
    assert_eq!(snapshot.len(), 1);
    let id = flea::json::field_usize(&snapshot[0], "id").unwrap();
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[32768]}}"#),
    );
    assert_eq!(
        sent(),
        [format!(
            r#"{{"c":"menuaction","op":"prepareDelete","id":{id}}}"#
        )]
    );
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"prepareDelete","ok":false,"error":"Read-only file system"}}"#
        ),
    );
    assert_eq!(
        sent(),
        [format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)]
    );
    assert!(!shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
