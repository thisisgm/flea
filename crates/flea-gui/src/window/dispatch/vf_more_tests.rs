//! Controlled replies for the VF-MORE rename race and live presentation updates.
use super::{
    rename_tests::{close, draw, receive, sent},
    test_shell::{land_on, open_root, row, set_cursor},
};
use crate::state::Shell;
use crate::{
    backend::{Backend, Event},
    window::navigate::Step,
};
use flea::protocol::Reply;
use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

fn far_ready(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_root(cx, "default");
    super::test_shell::land(&shell, cx, "/files", vec![row("f0000.txt", false)], 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.step(0, Step::Refresh(None), window, cx), Ok(true));
        })
    });
    land_on(
        &shell,
        cx,
        0,
        "/files",
        (0..320)
            .map(|n| row(&format!("f{n:04}.txt"), false))
            .collect(),
        1800,
        2,
    );
    shell.read_with(cx, |shell, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        assert_eq!(
            (pane.held.start, pane.held.rows.len(), pane.held.total),
            (0, 320, 1800)
        );
        assert!(!pane.busy());
        assert!(pane.requests.outstanding.is_none());
    });
    (shell, cx)
}

#[gpui::test]
fn vf_more_far_select_locates_a_target_missing_from_the_landed_window(cx: &mut TestAppContext) {
    let (shell, cx) = far_ready(cx);
    let (backend, lines) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    let mut lines: super::rename_tests::Lines = Box::new(lines);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.step(
                    0,
                    Step::Refresh(Some("/files/zz-last.txt".into())),
                    window,
                    cx
                ),
                Ok(true)
            );
        })
    });
    land_on(
        &shell,
        cx,
        0,
        "/files",
        (1..321)
            .map(|n| row(&format!("f{n:04}.txt"), false))
            .collect(),
        1800,
        3,
    );
    let requests = sent(&mut lines);
    assert!(
        requests
            .iter()
            .any(|line| line.contains("\"c\":\"locate\"") && line.contains("zz-last.txt")),
        "a missing held target must be located: {requests:?}"
    );
    let locate = requests
        .iter()
        .find(|line| line.contains("\"c\":\"locate\""))
        .unwrap();
    let id = flea::jsondoc::parse(locate)
        .unwrap()
        .get("id")
        .unwrap()
        .as_f64()
        .unwrap() as u64;
    receive(
        &shell,
        cx,
        Event::Located(crate::backend::Located {
            directory: "/files".into(),
            path: String::new(),
            index: -1,
            id: Some(id - 1),
            transfer: 0,
            ok: true,
            matches: vec![("/files/zz-last.txt".into(), 0)],
        }),
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
        0
    );
    receive(
        &shell,
        cx,
        Event::Located(crate::backend::Located {
            directory: "/files".into(),
            path: "/files/zz-last.txt".into(),
            index: 1799,
            id: Some(id),
            transfer: 0,
            ok: true,
            matches: vec![("/files/zz-last.txt".into(), 1799)],
        }),
    );
    let (start, count) = shell.read_with(cx, |shell, _| {
        let request = shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .requests
            .outstanding
            .unwrap();
        (request.start, request.count)
    });
    assert!(start <= 1799 && start + count > 1799 && count < 1800);
    receive(
        &shell,
        cx,
        Event::Reply(Reply::Rows {
            start,
            listing: 3,
            rows: (start..(start + count).min(1800))
                .map(|n| {
                    row(
                        &if n == 1799 {
                            "zz-last.txt".into()
                        } else {
                            format!("f{:04}.txt", n + 1)
                        },
                        false,
                    )
                })
                .collect(),
        }),
    );
    draw(cx);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 1799);
        assert_eq!(host.pane.held.row(1799).unwrap().name, "zz-last.txt");
        assert_eq!(host.selected, [1799]);
        assert!(host.nav.pending_select.is_none());
    });
    close(&shell, cx);
}

#[test]
fn vf_more_actions_default_on_preserves_saved_off_and_run_script_off() {
    for (input, actions_off) in [
        ("{}", false),
        (r#"{"stateVersion":4,"menu":{}}"#, false),
        (r#"{"menu":{"hidden":["actions"]}}"#, true),
        (
            r#"{"stateVersion":4,"menu":{"hidden":["actions","runScript"]}}"#,
            true,
        ),
    ] {
        let data = flea::uistate::from_file(input);
        let hidden = data
            .get("menu")
            .unwrap()
            .get("hidden")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(
            hidden.iter().any(|value| value.as_str() == Some("actions")),
            actions_off
        );
        let hidden_ids = hidden
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        let rows = flea::ui_logic_st1::menus::rows(&hidden_ids, false);
        let action = rows
            .iter()
            .find(|row| row.id == "actions")
            .expect("Actions owns its own settings switch");
        assert_eq!(action.on, !actions_off);
        assert!(!rows.iter().any(|row| row.id == "runScript"));
        if input == "{}" {
            assert!(hidden
                .iter()
                .any(|value| value.as_str() == Some("runScript")));
        }
    }
}

#[gpui::test]
fn vf_more_extracting_keeps_the_painted_cursor_accented(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    // The cursor keeps its accent only in the active window; a TestWindow starts inactive.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    super::test_shell::land(&shell, cx, "/files", vec![row("backup.tar.zst", false)], 1);
    draw(cx);
    receive(&shell, cx, Event::Operations(r#"{"t":"job","job":{"id":7,"kind":"extract","head":"Extracting","file":"backup.tar.zst","state":"running"}}"#.into()));
    receive(
        &shell,
        cx,
        Event::Operations(r#"{"t":"extractstarted","id":7,"n":1}"#.into()),
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    draw(cx);
    shell.read_with(cx, |shell, cx| {
        assert!(shell.overlay.is_some());
        let overlay = shell.overlay.as_ref().unwrap().read(cx);
        assert!(overlay.model.wire.transfer.extract && overlay.model.wire.transfer.running);
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        assert_eq!(list.drawn_cursor, Some(0));
        assert!(
            list.drawn_focused,
            "extracting keeps the archive's accent wash and 2px edge"
        );
    });
    close(&shell, cx);
}

#[gpui::test]
fn vf_more_rename_waits_for_an_older_reread_before_selecting_the_far_row(cx: &mut TestAppContext) {
    let (shell, cx) = far_ready(cx);
    set_cursor(&shell, cx, 0);
    let (backend, lines) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    let mut lines: super::rename_tests::Lines = Box::new(lines);
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_input("zz-last");
    cx.simulate_keystrokes("enter");
    assert!(sent(&mut lines)
        .iter()
        .any(|line| line.contains("\"c\":\"rename\"")));
    // An already requested re-read lands after the filesystem write, before the rename's own refresh.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.step(0, Step::Reread, window, cx), Ok(true));
        })
    });
    sent(&mut lines);
    receive(
        &shell,
        cx,
        Event::Producer(r#"{"t":"renamed","ok":true,"path":"/files/zz-last.txt"}"#.into()),
    );
    for reply in [
        Reply::Listed {
            path: "/files".into(),
            total: 1800,
        },
        Reply::Rows {
            start: 0,
            listing: 3,
            rows: (1..321)
                .map(|n| row(&format!("f{n:04}.txt"), false))
                .collect(),
        },
    ] {
        receive(&shell, cx, Event::Reply(reply));
    }
    let requests = sent(&mut lines);
    assert!(
        requests
            .iter()
            .any(|line| line.contains("\"c\":\"list\"") && line.contains("/files/zz-last.txt")),
        "the rename owes a fresh listing with its select after the older rows: {requests:?}"
    );
    for reply in [
        Reply::Listed {
            path: "/files".into(),
            total: 1800,
        },
        Reply::Rows {
            start: 1480,
            listing: 4,
            rows: (1480..1800)
                .map(|n| {
                    row(
                        &if n == 1799 {
                            "zz-last.txt".into()
                        } else {
                            format!("f{:04}.txt", n + 1)
                        },
                        false,
                    )
                })
                .collect(),
        },
    ] {
        receive(&shell, cx, Event::Reply(reply));
    }
    draw(cx);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!((host.pane.cursor, host.pane.held.start), (1799, 1480));
        assert_eq!(
            host.pane.held.row(host.pane.cursor).unwrap().name,
            "zz-last.txt"
        );
        assert_eq!(host.selected, [1799]);
        assert!(host.nav.pending_select.is_none());
    });
    close(&shell, cx);
}

#[gpui::test]
fn vf_more_rename_field_right_click_opens_edit_actions(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    super::test_shell::land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    cx.simulate_keystrokes("f2");
    draw(cx);
    let frame = shell.read_with(cx, |shell, _| {
        shell.rename.edit.as_ref().unwrap().line.frame.unwrap()
    });
    cx.simulate_mouse_down(frame.center(), MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(frame.center(), MouseButton::Right, Modifiers::none());
    draw(cx);
    let entries = crate::window::u07_tests::read(&shell, cx, "contextMenuEntries", &[]);
    for label in ["Undo", "Cut", "Copy", "Paste", "Select All"] {
        assert!(
            entries.contains(label),
            "rename field menu misses {label}: {entries}"
        );
    }
    assert!(!entries.contains("Open With"));
    assert!(shell.read_with(cx, |shell, _| shell
        .rename
        .edit
        .as_ref()
        .unwrap()
        .entry
        .editing));
    let cut = entries.split('|').position(|label| label == "Cut").unwrap();
    let centre =
        crate::window::u07_tests::read(&shell, cx, "contextMenuRowCentre", &[&cut.to_string()]);
    crate::window::u07_tests::press(cx, &centre, 1);
    draw(cx);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .rename
            .edit
            .as_ref()
            .unwrap()
            .entry
            .text
            .clone()),
        ".txt"
    );
    let frame = shell.read_with(cx, |shell, _| {
        shell.rename.edit.as_ref().unwrap().line.frame.unwrap()
    });
    cx.simulate_mouse_down(frame.center(), MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(frame.center(), MouseButton::Right, Modifiers::none());
    draw(cx);
    let centre = crate::window::u07_tests::read(&shell, cx, "contextMenuRowCentre", &["0"]);
    crate::window::u07_tests::press(cx, &centre, 1);
    draw(cx);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .rename
            .edit
            .as_ref()
            .unwrap()
            .entry
            .text
            .clone()),
        "notes.txt"
    );
    close(&shell, cx);
}
