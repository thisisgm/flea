//! Mounted listing regressions for Trash landing, refusal and Escape ownership.
use super::{
    fileops_tests::{reply, wire},
    test_shell::{land, lists, open_root_with, row, set_cursor},
};
use crate::window::navigate::Step;
use gpui::TestAppContext;

pub(super) const SMB: &str = "/run/user/1000/gvfs/smb-share:server=host,share=docs";

#[gpui::test]
fn uish_def1_phone_d_refuses_before_arming(cx: &mut TestAppContext) {
    for path in [SMB, "/run/user/1000/gvfs/mtp:host=Phone/DCIM"] {
        let (shell, cx) = super::test_shell::open_root_at(cx, path);
        land(&shell, cx, path, vec![row("notes.txt", false)], 1);
        let mut sent = wire(&shell, cx);
        cx.simulate_keystrokes("d");
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.operations.fileops.armed_at(),
                0,
                "no Trash key pair on {path}"
            );
            assert!(
                shell.keyboard.pending().is_none(),
                "no pending key pair on {path}"
            );
            let text = shell.status_messages(cx, |messages, _| messages.shown().text);
            assert!(text.contains("no Trash"), "{text}");
        });
        cx.simulate_keystrokes("d");
        assert!(
            sent().is_empty(),
            "dd requests neither Trash nor permanent deletion"
        );
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
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
        ("default", "shift-delete"),
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
fn bfx325_no_trash_refusal_keeps_explicit_delete_and_a_denied_review_never_deletes(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = super::test_shell::open_root_at(cx, SMB);
    land(&shell, cx, SMB, vec![row("notes.txt", false)], 1);
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("d");
    assert!(sent().is_empty(), "d refuses before arming");
    let text = shell.update(cx, |shell, cx| {
        shell.status_messages(cx, |messages, _| messages.shown().text)
    });
    assert!(text.contains("no Trash"), "{text}");
    cx.simulate_keystrokes("d");
    assert!(sent().is_empty(), "dd cannot request permanent deletion");
    cx.simulate_keystrokes("delete");
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

fn ten() -> Vec<flea::protocol::RowFacts> {
    (1..=10).map(|n| row(&format!("f{n}.txt"), false)).collect()
}

fn line_text(
    shell: &gpui::Entity<crate::state::Shell>,
    cx: &mut gpui::VisualTestContext,
) -> String {
    shell.update(cx, |shell, cx| {
        let catalog = shell.locale.catalog.clone();
        shell.status_messages(cx, |messages, _| messages.transient_text(&catalog))
    })
}

#[gpui::test]
fn ey144_escape_after_d_on_a_landed_row_only_disarms(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(
        cx,
        r#""keys":"default","view":"list","escapeUp":true"#,
        Some("/files/alpha.txt"),
    );
    land(
        &shell,
        cx,
        "/files",
        vec![row("alpha.txt", false), row("beta.txt", false)],
        1,
    );
    let selected = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().selected.clone()
    });
    assert_eq!(selected, vec![0], "the scene's landed row");
    let before = lists(&shell, cx);
    cx.simulate_keystrokes("d");
    cx.simulate_keystrokes("escape");
    assert_eq!(lists(&shell, cx), before, "Escape only cancels the armed d");
    assert_eq!(super::test_shell::asked(&shell, cx), "/files");
    cx.simulate_keystrokes("escape");
    assert_eq!(lists(&shell, cx), before + 1, "the next Escape climbs");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey179_first_d_on_a_marked_block_says_the_pair(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""keys":"default","view":"list""#, None);
    land(&shell, cx, "/files", ten(), 1);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("v shift-j");
    let selected = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().selected.clone()
    });
    assert_eq!(selected, vec![1, 2]);
    cx.simulate_keystrokes("d");
    assert_eq!(
        line_text(&shell, cx),
        "Press d again to move them to Trash, or press Delete on its own."
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey115_backspace_in_an_empty_filter_after_ctrl_a_stays_in_the_folder(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""keys":"default","view":"list""#, None);
    land(&shell, cx, "/files", ten(), 1);
    let before = lists(&shell, cx);
    for keys in [
        "/ ctrl-a backspace",
        "escape / n t ctrl-a escape / ctrl-a backspace",
    ] {
        cx.simulate_keystrokes(keys);
        let (place, up, typing, query) = shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let filter = &host.filter.owner;
            (
                host.pane.place().to_owned(),
                filter.up(),
                filter.typing(),
                filter.query().to_owned(),
            )
        });
        assert_eq!(
            (place.as_str(), lists(&shell, cx)),
            ("/files", before),
            "{keys}"
        );
        assert!(
            up && typing && query.is_empty(),
            "{keys}: the empty strip keeps its caret"
        );
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey194_dd_selects_the_row_that_takes_the_trashed_ones_place(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""keys":"default","view":"list""#, None);
    let mut rows = ten();
    land(&shell, cx, "/files", rows.clone(), 1);
    set_cursor(&shell, cx, 2);
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("d d");
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[2],"menuId":0,"listing":1}"#]
    );
    reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
    rows.remove(2);
    land(&shell, cx, "/files", rows, 2);
    let (cursor, selected) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.pane.cursor, host.selected.clone())
    });
    assert_eq!(
        (cursor, selected),
        (2, vec![2]),
        "f4.txt takes f3.txt's place, selected"
    );
    cx.simulate_keystrokes("down");
    let selected = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().selected.clone()
    });
    assert_eq!(selected, vec![3], "the next key acts on the lone selection");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey194_a_trash_answered_during_a_watch_reread_still_selects_the_replacement(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_root_with(cx, r#""keys":"default","view":"list""#, None);
    let mut rows = ten();
    land(&shell, cx, "/files", rows.clone(), 1);
    set_cursor(&shell, cx, 2);
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("d d");
    assert_eq!(sent().len(), 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let _ = shell.step(0, Step::Refresh(None), window, cx);
        })
    });
    reply(&shell, cx, r#"{"t":"trashed","ok":1,"failed":0}"#);
    rows.remove(2);
    land(&shell, cx, "/files", rows.clone(), 2);
    land(&shell, cx, "/files", rows, 3);
    let (cursor, selected) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.pane.cursor, host.selected.clone())
    });
    assert_eq!(
        (cursor, selected),
        (2, vec![2]),
        "the replacement row stays selected"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
