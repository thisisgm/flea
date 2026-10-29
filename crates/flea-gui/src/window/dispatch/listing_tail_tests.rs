//! Extracted mounted cases; shared fixture ownership stays in the parent test module.
use super::*;

#[gpui::test]
fn a_reload_that_changed_rows_says_how_many(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    cx.simulate_keystrokes("f5");
    // The backend's added plus removed count arrives on the reload's listed line.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, crate::backend::Event::ReloadChanged(2), window, cx)
        })
    });
    let mut rows = home();
    rows.push(row("todo.md", false));
    land(&shell, cx, "/files", rows.clone(), 2);
    assert_eq!(notice(&shell, cx), "Reloaded · 2 items changed");
    shell.update(cx, |shell, cx| {
        shell.status_messages(cx, |messages, now| messages.dismiss(now))
    });
    // With no count on the line, the net change is the count.
    cx.simulate_keystrokes("f5");
    let mut more = rows;
    more.push(row("zz.txt", false));
    more.push(row("zzz.txt", false));
    land(&shell, cx, "/files", more.clone(), 3);
    assert_eq!(notice(&shell, cx), "Reloaded · 2 items changed");
    shell.update(cx, |shell, cx| {
        shell.status_messages(cx, |messages, now| messages.dismiss(now))
    });
    // A reload that changed nothing says nothing.
    cx.simulate_keystrokes("f5");
    land(&shell, cx, "/files", more, 4);
    assert_eq!(notice(&shell, cx), "");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_failed_reload_leaves_no_reload_owed(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    cx.simulate_keystrokes("f5");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, crate::backend::Event::ReloadChanged(2), window, cx);
            shell.receive(
                0,
                crate::backend::Event::Refused {
                    operation: "scan".into(),
                    path: "/files".into(),
                    message: "Permission denied".into(),
                    mode: 0,
                },
                window,
                cx,
            );
        })
    });
    let owed = shell.read_with(cx, |shell, _| {
        let nav = &shell.panes[0].as_ref().unwrap().nav;
        (nav.reload_from.clone(), nav.reload_changed)
    });
    assert_eq!(owed, (None, None), "a failed reload owes no notice");
    shell.update(cx, |shell, cx| {
        shell.status_messages(cx, |messages, now| messages.dismiss(now))
    });
    // A later listing of the same folder is no reload, whatever its count.
    let mut rows = home();
    rows.push(row("todo.md", false));
    land(&shell, cx, "/files", rows, 2);
    assert_eq!(notice(&shell, cx), "");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// EY-49: Nav.js applyPendingSelect lands the asked row as a lone selection, so v keeps it and v j v marks two.
#[gpui::test]
fn a_select_launch_lands_a_lone_selection_that_v_keeps(cx: &mut TestAppContext) {
    let (shell, cx) = open_with(
        cx,
        r#""keys":"default","view":"list""#,
        Some("/files/alpha.txt"),
    );
    let rows = ["alpha.txt", "beta.md", "gamma.txt"]
        .iter()
        .map(|name| row(name, false))
        .collect();
    land(&shell, cx, "/files", rows, 1);
    assert_eq!(selected(&shell, cx), vec![0], "the asked row is selected");
    cx.simulate_keystrokes("v j v");
    assert_eq!(
        selected(&shell, cx),
        vec![0, 1],
        "v keeps the landed row as a mark, then marks the next"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_select_launch_selection_follows_a_plain_move(cx: &mut TestAppContext) {
    let (shell, cx) = open_with(
        cx,
        r#""keys":"default","view":"list""#,
        Some("/files/alpha.txt"),
    );
    let rows = ["alpha.txt", "beta.md", "gamma.txt"]
        .iter()
        .map(|name| row(name, false))
        .collect();
    land(&shell, cx, "/files", rows, 1);
    cx.simulate_keystrokes("j");
    assert_eq!(
        selected(&shell, cx),
        vec![1],
        "a lone landed selection follows the cursor"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
