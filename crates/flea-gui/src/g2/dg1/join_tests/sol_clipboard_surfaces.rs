//! Clipboard selection is shared between panes; menus and footer use that same state.
use super::{changed, copy, edit, frame, key, open, reply};
use crate::{
    backend::Backend,
    window::{pane::PaneHost, startup::View},
};
use gpui::TestAppContext;

#[gpui::test]
fn sol_clipboard_pastes_copy_and_cut_into_the_other_pane(cx: &mut TestAppContext) {
    for moving in [false, true] {
        let (handle, mut left_lines) = open(cx);
        let (backend, mut right_lines) = Backend::test_lines();
        edit(cx, handle, |shell, window, cx| {
            let mut host = PaneHost::new("/dest".into(), None, 620., &shell.locale);
            host.pane.use_executor(cx.background_executor().clone());
            host.backend = Some(backend);
            shell.panes[1] = Some(host);
            for line in [
                r#"{"t":"listed","path":"/dest","n":0}"#,
                r#"{"t":"rows","start":0,"listing":1,"rows":[]}"#,
            ] {
                shell.receive(1, Backend::test_reply_line(line), window, cx);
            }
            shell.view = View::Dual;
        });
        frame(cx, handle);
        copy(cx, handle, moving);
        reply(
            cx,
            handle,
            r#"{"t":"clip","op":"set","ok":true,"token":"T"}"#,
        );
        edit(cx, handle, |shell, window, cx| {
            let op = if moving { "cut" } else { "copy" };
            let line = format!(
                r#"{{"t":"clip","op":"changed","clip":"{op}","paths":["/files/a.txt"],"token":"T"}}"#
            );
            shell.receive(1, Backend::test_reply_line(&line), window, cx);
            shell.focus_side = 1;
        });
        left_lines();
        std::iter::from_fn(&mut right_lines).for_each(drop);
        key(cx, handle, "ctrl-v");
        let sent: Vec<_> = std::iter::from_fn(&mut right_lines).collect();
        assert!(
            sent.iter().any(|line| line.contains("collisions")
                && line.contains("/files/a.txt")
                && line.contains("/dest")),
            "{sent:?}"
        );
        edit(cx, handle, |shell, window, cx| {
            shell.receive(
                1,
                Backend::test_reply_line(
                    r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
                ),
                window,
                cx,
            );
        });
        let sent: Vec<_> = std::iter::from_fn(&mut right_lines).collect();
        let op = if moving { "move" } else { "copy" };
        assert!(sent
            .iter()
            .any(|line| line.contains("transfer") && line.contains(op)));
        assert_eq!(sent.iter().any(|line| line.contains("clipClear")), moving);
        assert!(
            left_lines().is_empty(),
            "the destination backend owns the operation"
        );
    }
}

#[gpui::test]
fn sol_clipboard_menu_paste_follows_empty_files_and_failed_watch(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx);
    for state in ["empty", "files", "failed"] {
        if state == "files" {
            changed(cx, handle, "copy", "/other/a", "F");
        } else {
            reply(
                cx,
                handle,
                r#"{"t":"clip","op":"changed","clip":"none","paths":[],"token":""}"#,
            );
            if state == "failed" {
                reply(
                    cx,
                    handle,
                    r#"{"t":"clip","op":"changed","error":"watch down"}"#,
                );
            }
        }
        key(cx, handle, "m");
        edit(cx, handle, |shell, _, _| {
            let entries = shell.menu_entries();
            let paste = entries
                .iter()
                .find(|entry| entry.action == "paste")
                .expect("Paste row");
            assert_eq!(paste.disabled, state == "empty", "{state}");
        });
        key(cx, handle, "escape");
    }
}

#[gpui::test]
fn sol_clipboard_copy_cut_footer_counts(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx);
    for moving in [false, true] {
        copy(cx, handle, moving);
        let status = edit(cx, handle, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().status.to_string()
        });
        assert_eq!(
            status,
            if moving {
                "Cut 1 item"
            } else {
                "Copied 1 item"
            }
        );
        reply(
            cx,
            handle,
            r#"{"t":"clip","op":"set","ok":true,"token":"T"}"#,
        );
    }
}

#[gpui::test]
fn sol_clipboard_disconnect_reconciles_the_other_panes_deferred_offer(cx: &mut TestAppContext) {
    for disconnect_source in [false, true] {
        let (handle, mut lines) = open(cx);
        let (backend, _) = Backend::test_lines();
        edit(cx, handle, |shell, _, _| {
            let mut host = PaneHost::new("/other".into(), None, 620., &shell.locale);
            host.backend = Some(backend);
            shell.panes[1] = Some(host);
        });
        copy(cx, handle, true);
        edit(cx, handle, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 1
        });
        key(cx, handle, "ctrl-x");
        reply(cx, handle, r#"{"t":"paths","paths":["/files/b.txt"]}"#);
        // This newer offer matches the older pending set, so token reconciliation defers it.
        changed(cx, handle, "cut", "/files/a.txt", "foreign-A");
        edit(cx, handle, |shell, _, _| {
            shell.clipboard_backend_ended(usize::from(!disconnect_source))
        });
        if !disconnect_source {
            for token in ["own-A", "own-B"] {
                reply(
                    cx,
                    handle,
                    &format!(r#"{{"t":"clip","op":"set","ok":true,"token":"{token}"}}"#),
                );
            }
        }
        let clip = edit(cx, handle, |shell, _, _| {
            shell.clipboard.board.current().clone()
        });
        assert_eq!(
            clip.paths,
            ["/files/a.txt"],
            "the latest observed selection survives either backend ending"
        );
        assert_eq!(clip.token, "foreign-A");
        if !disconnect_source {
            lines();
            key(cx, handle, "ctrl-v");
            assert!(lines()
                .iter()
                .any(|line| line.contains("collisions") && line.contains("/files/a.txt")));
        }
    }
}

#[gpui::test]
fn sol_clipboard_open_menu_tracks_foreign_changes(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 1];
        shell.launch.settings.data =
            flea::jsondoc::parse(r#"{"keys":"default","menu":{"hidden":[]}}"#).unwrap();
    });
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"none","paths":[],"token":""}"#,
    );
    super::super::menu(cx, handle, &mut lines, "m");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(
            crate::pane::swap::MIN_SHOW_MS + 1,
        ));
    cx.run_until_parked();
    frame(cx, handle);
    let disabled = |cx: &mut TestAppContext| {
        edit(cx, handle, |shell, _, _| {
            let logical = shell
                .menu_entries()
                .iter()
                .find(|entry| entry.action == "paste")
                .unwrap()
                .disabled;
            let menu = shell.menu.menu.borrow();
            let drawn = menu
                .card
                .as_ref()
                .expect("the menu is mounted")
                .entries
                .iter()
                .find(|entry| entry.action == "paste")
                .unwrap()
                .disabled;
            assert_eq!(
                logical, drawn,
                "the painted menu and keyboard facts refresh together"
            );
            drawn
        })
    };
    assert!(disabled(cx));
    changed(cx, handle, "copy", "/other/new.txt", "foreign");
    assert!(
        !disabled(cx),
        "an already open menu must enable Paste for a new file offer"
    );
    assert!(
        edit(cx, handle, |shell, _, _| shell
            .menu
            .context
            .clipboard_available),
        "keyboard flyout admission uses the same live clipboard facts"
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"none","paths":[],"token":""}"#,
    );
    assert!(disabled(cx));
    assert!(!edit(cx, handle, |shell, _, _| shell
        .menu
        .context
        .clipboard_available));
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","error":"watch down"}"#,
    );
    assert!(
        !disabled(cx),
        "the open menu allows the failed watch's fallback read"
    );
}
