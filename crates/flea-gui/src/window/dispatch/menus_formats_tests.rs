//! Extracted mounted cases; shared fixture ownership stays in the parent test module.
use super::*;

#[gpui::test]
fn compress_and_extract_follow_the_backends_formats_reply(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = open(cx, "default");
    let mut rows = home();
    rows.push(entry("pack.zip", FILE));
    land(&shell, cx, "/files", rows, 1);
    reply(
        &shell,
        cx,
        r#"{"t":"formats","archive":["zip","tar.gz"],"convert":true,"extract":{"archive":true,"sevenZip":false,"zip":true}}"#,
    );
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 2
        })
    });
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[FILE]);
    let shown = actions(&shell, cx);
    assert!(shown.contains(&"compress".to_owned()), "{shown:?}");
    assert!(
        !shown.contains(&"extract".to_owned()),
        "a text file has no Extract"
    );
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 4
        })
    });
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[FILE]);
    assert!(actions(&shell, cx).contains(&"extract".to_owned()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn compare_shows_only_for_two_regular_files_and_routes_both_paths(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let mark = |shell: &Entity<Shell>, cx: &mut VisualTestContext, rows: Vec<usize>| {
        cx.update(|_, cx| {
            shell.update(cx, |shell, _| {
                let host = shell.panes[0].as_mut().unwrap();
                host.pane.cursor = rows[0];
                host.selected = rows;
            })
        });
    };
    // One file, then a folder with a file: no Compare.
    for (rows, kinds) in [(vec![2], vec![FILE]), (vec![1, 2], vec![DIR, FILE])] {
        mark(&shell, cx, rows);
        cx.simulate_keystrokes("m");
        answer(&shell, cx, &kinds);
        assert!(!actions(&shell, cx).contains(&"compare".to_owned()));
        cx.simulate_keystrokes("escape");
    }
    mark(&shell, cx, vec![2, 3]);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[FILE, FILE]);
    let shown = actions(&shell, cx);
    // Menu.js over two files: Compare and the many-item rows; Duplicate acts on one item only.
    for row in ["compare", "rename", "cut", "copy", "trash"] {
        assert!(shown.contains(&row.to_owned()), "{row} in {shown:?}");
    }
    assert!(!shown.contains(&"duplicate".to_owned()), "{shown:?}");
    let entries = read(&shell, cx, "contextMenuEntries", &[]);
    assert!(entries.contains("Rename 2 Items"), "{entries}");
    let handed = Rc::new(RefCell::new(Vec::new()));
    let sink = handed.clone();
    let _observer = cx.update(|_, cx| {
        cx.subscribe(&shell, move |_, chosen: &Chosen, _| {
            sink.borrow_mut()
                .push((chosen.action.clone(), chosen.paths.clone()))
        })
    });
    let (backend, mut sent) = crate::backend::Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    choose(&shell, cx, "compare");
    validated(
        &shell,
        cx,
        "compare",
        &["/files/notes.txt", "/files/todo.md"],
    );
    // The menus family claims Compare and opens its card on the validated pair; nothing is handed on.
    assert!(handed.borrow().is_empty(), "{:?}", handed.borrow());
    let lines: Vec<String> = std::iter::from_fn(&mut sent).collect();
    let pair = r#""a":"/files/notes.txt","b":"/files/todo.md""#;
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""c":"compare""#) && line.contains(pair)),
        "{lines:?}"
    );
    assert_eq!(read(&shell, cx, "previewKind", &[]), "compare");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn the_providers_formats_reply_under_the_menu_id_brings_compress(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 2
        })
    });
    cx.simulate_keystrokes("m");
    let id = captured(&shell, cx);
    answer(&shell, cx, &[FILE]);
    assert!(
        !actions(&shell, cx).contains(&"compress".to_owned()),
        "no formats answer yet"
    );
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"formats","id":{id},"archive":["zip"],"convert":true,"extract":{{"archive":true,"sevenZip":false,"zip":true}},"providers":{{}}}}"#
        ),
    );
    assert!(actions(&shell, cx).contains(&"compress".to_owned()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
