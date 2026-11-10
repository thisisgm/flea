//! Trash uses the window journal and re-reads its rows after completion without relying on a watch.
use super::join_tests::{answer, call, draw, opened, root, shell, state};
use crate::{backend::Backend, list::bench::real_cx};
use flea::jsondoc::Json;
use gpui::{Modifiers, MouseButton, VisualTestContext};

#[test]
fn trash_undo_keys_reach_the_window_journal() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    let (backend, mut read) = Backend::test_lines();
    shell(&mut cx, window, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend);
    });
    for (preset, chord, action) in [
        ("default", "ctrl-z", "undo"),
        ("default", "ctrl-shift-z", "redo"),
        ("windows", "ctrl-y", "redo"),
    ] {
        shell(&mut cx, window, |shell, _, _| {
            shell.launch.settings.data =
                flea::uistate::from_file(&format!(r#"{{"keys":"{preset}"}}"#));
        });
        cx.simulate_keystrokes(window.into(), chord);
        draw(&mut cx, window);
        let lines: Vec<_> = std::iter::from_fn(&mut read).collect();
        assert_eq!(
            lines,
            [format!(r#"{{"c":"{action}"}}"#)],
            "{preset} {chord} runs the journal once, with no premature Trash refresh"
        );
        assert_eq!(
            state(&mut cx, window).get("opened"),
            Some(&Json::Bool(true))
        );
    }
}

#[test]
fn trash_undo_receipts_refresh_the_rows_without_a_watch() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    let (backend, mut read) = Backend::test_lines();
    shell(&mut cx, window, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend);
    });
    for (receipt, total, rows) in [
        ("undone", 0, "[]"),
        (
            "redone",
            1,
            r#"[{"uri":"trash:///alpha","original":"/private/alpha.txt","identity":"alpha-identity","size":6}]"#,
        ),
    ] {
        shell(&mut cx, window, |shell, window, cx| {
            shell.receive(
                0,
                Backend::test_reply_line(&format!(r#"{{"t":"{receipt}","op":"trash","ok":true}}"#)),
                window,
                cx,
            );
        });
        draw(&mut cx, window);
        let lines: Vec<_> = std::iter::from_fn(&mut read).collect();
        assert_eq!(
            lines
                .iter()
                .filter(
                    |line| line.contains(r#""c":"trashbrowse""#) && line.contains(r#""op":"list""#)
                )
                .count(),
            1,
            "{receipt} owes one Trash listing after completion: {lines:?}"
        );
        answer(
            &mut cx,
            window,
            "list",
            &format!(r#", "total":{total},"start":0,"present":[],"rows":{rows}"#),
        );
        answer(&mut cx, window, "summary", r#", "bytes":0,"partial":false"#);
        draw(&mut cx, window);
        let view = state(&mut cx, window);
        assert_eq!(view.get("total").and_then(Json::as_f64), Some(total as f64));
        assert_eq!(view.get("busy"), Some(&Json::Bool(false)));
        assert_eq!(view.get("opened"), Some(&Json::Bool(true)));
        let (primary, message) = shell(&mut cx, window, |shell, _, _| {
            let catalog = &shell.locale.catalog;
            if receipt == "undone" {
                let line = flea::ui_logic_file_ops::lines::undone(catalog, "trash");
                (line.clone(), line)
            } else {
                let line = flea::ui_logic_file_ops::lines::redone(catalog, "trash");
                let receipt =
                    flea::ui_logic_file_ops::lines::with_undo(catalog, "default", line.clone());
                (line, receipt)
            }
        });
        assert_eq!(call(&mut cx, window, "lastMessage", &[]), message);
        assert_eq!(call(&mut cx, window, "statusPrimary", &[]), primary);
        let retained = shell(&mut cx, window, |shell, _, cx| {
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            footer
                .line
                .read(cx)
                .messages()
                .transient_text(&shell.locale.catalog)
        });
        assert_eq!(
            retained, message,
            "the folder queue retains its receipt and expiry"
        );
        let _: Vec<_> = std::iter::from_fn(&mut read).collect();
    }
}

#[test]
fn trash_undo_receipt_target_runs_the_window_journal() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    let (backend, mut read) = Backend::test_lines();
    shell(&mut cx, window, |shell, _, cx| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend);
        let catalog = &shell.locale.catalog;
        let text = flea::ui_logic_file_ops::lines::with_undo(
            catalog,
            "default",
            flea::ui_logic_file_ops::lines::redone(catalog, "trash"),
        );
        cx.emit(crate::g2::file_ops::join::OperationUpdate {
            side: 0,
            said: vec![crate::g2::file_ops::model::Said {
                text,
                error: false,
                detail: String::new(),
                behind: false,
            }],
            sticky: String::new(),
            spent_cut: false,
        });
    });
    draw(&mut cx, window);
    draw(&mut cx, window);
    {
        let mut visual = VisualTestContext::from_window(window.into(), &cx);
        let target = visual
            .debug_bounds("fb1-undo")
            .expect("redo offers the same undo target as a folder");
        let at = target.center();
        visual.simulate_mouse_move(at, None, Modifiers::none());
        visual.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        visual.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        visual.run_until_parked();
    }
    let lines: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert_eq!(
        lines,
        [r#"{"c":"undo"}"#],
        "Trash's receipt undo target runs the window journal"
    );
}
