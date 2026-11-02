use crate::{
    backend::{Backend, Event},
    navigation::folder_state::Folders,
    state::Shell,
    window::{navigate::Step, u07_tests::read},
};
use flea::{
    jsondoc::Json,
    protocol::{Reply, RowFacts},
};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn land(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    path: &str,
    listing: usize,
    names: &[&str],
) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            for reply in [
                Reply::Listed {
                    path: path.into(),
                    total: names.len(),
                },
                Reply::Rows {
                    start: 0,
                    rows: names
                        .iter()
                        .map(|name| RowFacts {
                            name: (*name).into(),
                            dir: true,
                            mode: 0o40755,
                            ..Default::default()
                        })
                        .collect(),
                    listing,
                },
            ] {
                shell.receive(0, Event::Reply(reply), window, cx);
            }
        })
    });
    cx.run_until_parked();
}

fn request(requests: &mut impl FnMut() -> Option<String>, command: &str) -> Json {
    while let Some(line) = requests() {
        let data = flea::jsondoc::parse(&line).unwrap();
        if data.get("c").and_then(Json::as_str) == Some(command) {
            return data;
        }
    }
    panic!("mounted keyboard input did not send {command}");
}

fn mount(
    backend: Backend,
    cx: &mut TestAppContext,
    remember: bool,
) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        let mut shell = crate::state::tests::shell_with(backend, cx);
        let data = flea::uistate::from_file(&format!(r#"{{"rememberSort":{remember}}}"#));
        *shell.panes[0].as_ref().unwrap().nav.folders.borrow_mut() =
            Folders::from_settings(&data, Vec::new());
        shell.launch.settings.data = data;
        shell
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            window.focus(&shell.focus, cx);
            assert_eq!(
                shell.step(0, Step::Open("/work".into()), window, cx),
                Ok(true)
            );
        })
    });
    land(&shell, cx, "/work", 3, &["A", "Empty"]);
    (shell, cx)
}

#[gpui::test]
fn ey139_remember_off_keeps_keyboard_sort_after_enter_and_parent(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let (shell, cx) = mount(backend, cx, false);
    while requests().is_some() {}
    cx.simulate_keystrokes("s");
    let sort = request(&mut requests, "sort");
    assert_eq!(sort.get("by").and_then(Json::as_str), Some("size"));
    assert_eq!(sort.get("desc").and_then(Json::as_bool), Some(false));
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .nav
                .writer
                .borrow()
                .sort_writes(),
            0
        );
    });
    land(&shell, cx, "/work", 4, &["Empty", "A"]);
    assert_eq!(read(&shell, cx, "sortMark", &[]), "size:asc");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let data = flea::uistate::patched(
                &shell.launch.settings.data,
                &flea::jsondoc::parse(r#"{"langMarks":false}"#).unwrap(),
            )
            .unwrap();
            shell.apply_settings(data, window, cx);
        })
    });
    cx.simulate_keystrokes("down enter");
    let list = request(&mut requests, "list");
    assert_eq!(list.get("path").and_then(Json::as_str), Some("/work/A"));
    assert_eq!(
        list.get("by").and_then(Json::as_str),
        Some("size"),
        "enter must keep the chosen session sort"
    );
    land(&shell, cx, "/work/A", 5, &["child"]);
    assert_eq!(read(&shell, cx, "sortMark", &[]), "size:asc");
    cx.simulate_keystrokes("h");
    let list = request(&mut requests, "list");
    assert_eq!(list.get("path").and_then(Json::as_str), Some("/work"));
    assert_eq!(list.get("by").and_then(Json::as_str), Some("size"));
    land(&shell, cx, "/work", 6, &["Empty", "A"]);
    assert_eq!(read(&shell, cx, "path", &[]), "/work");
    assert_eq!(read(&shell, cx, "sortMark", &[]), "size:asc");
    assert_eq!(read(&shell, cx, "cursor", &[]), "1");
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.held.rows[0].name, "Empty");
        assert_eq!(host.nav.folders.borrow().sort_count(), 0);
        assert_eq!(host.nav.writer.borrow().sort_writes(), 1);
    });
    cx.simulate_keystrokes("shift-s");
    let sort = request(&mut requests, "sort");
    assert_eq!(sort.get("by").and_then(Json::as_str), Some("size"));
    assert_eq!(sort.get("desc").and_then(Json::as_bool), Some(true));
    land(&shell, cx, "/work", 7, &["A", "Empty"]);
    assert_eq!(read(&shell, cx, "sortMark", &[]), "size:desc");
    cx.simulate_keystrokes("s");
    let sort = request(&mut requests, "sort");
    assert_eq!(sort.get("by").and_then(Json::as_str), Some("mtime"));
    land(&shell, cx, "/work", 8, &["A", "Empty"]);
    shell.read_with(cx, |shell, _| {
        let saved = shell.launch.settings.data.get("sort").unwrap();
        assert_eq!(saved.get("key").and_then(Json::as_str), Some("date"));
        assert_eq!(saved.get("reverse").and_then(Json::as_bool), Some(false));
        let folders = Folders::from_settings(&shell.launch.settings.data, Vec::new());
        assert_eq!(folders.order_for("/work"), folders.order_for("/work/A"));
        assert_eq!(folders.peek_order("/work/A").unwrap().key, "mtime");
        let writer = shell.panes[0].as_ref().unwrap().nav.writer.borrow();
        let patches: Vec<_> = writer
            .patches
            .iter()
            .filter_map(|patch| {
                let data = flea::jsondoc::parse(patch).unwrap();
                data.get("sort").cloned()
            })
            .collect();
        assert_eq!(patches.len(), 3);
        assert_eq!(patches.last(), Some(saved));
    });
}

#[gpui::test]
fn ey139_remember_on_keeps_each_folder_sort_and_leaves_global_untouched(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let (shell, cx) = mount(backend, cx, true);
    while requests().is_some() {}
    cx.simulate_keystrokes("s");
    request(&mut requests, "sort");
    land(&shell, cx, "/work", 4, &["Empty", "A"]);
    cx.simulate_keystrokes("down enter");
    let list = request(&mut requests, "list");
    assert_eq!(list.get("path").and_then(Json::as_str), Some("/work/A"));
    assert_eq!(list.get("by").and_then(Json::as_str), Some("name"));
    land(&shell, cx, "/work/A", 5, &["child"]);
    assert_eq!(read(&shell, cx, "sortMark", &[]), "name:asc");
    cx.simulate_keystrokes("h");
    let list = request(&mut requests, "list");
    assert_eq!(list.get("by").and_then(Json::as_str), Some("size"));
    land(&shell, cx, "/work", 6, &["Empty", "A"]);
    assert_eq!(read(&shell, cx, "sortMark", &[]), "size:asc");
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.nav.folders.borrow().global.key, "name");
        assert_eq!(host.nav.folders.borrow().sort_count(), 1);
        assert!(host
            .nav
            .writer
            .borrow()
            .patches
            .iter()
            .all(|patch| { flea::jsondoc::parse(patch).unwrap().get("sort").is_none() }));
    });
}

#[path = "sort_atomic_tests.rs"]
mod atomic;

#[gpui::test]
fn uish_misc_showing_hidden_files_lands_the_cursor_on_row_zero(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let (shell, cx) = mount(backend, cx, false);
    while requests().is_some() {}
    cx.simulate_keystrokes("down");
    assert_eq!(read(&shell, cx, "cursor", &[]), "1");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let data = flea::uistate::patched(
                &shell.launch.settings.data,
                &flea::jsondoc::parse(r#"{"hidden":true}"#).unwrap(),
            )
            .unwrap();
            shell.apply_settings(data, window, cx);
        })
    });
    let list = request(&mut requests, "list");
    assert_eq!(list.get("hidden").and_then(Json::as_bool), Some(true));
    let listing = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.held.listing + 1
    });
    land(&shell, cx, "/work", listing, &[".dot", "A", "Empty"]);
    assert_eq!(
        read(&shell, cx, "cursor", &[]),
        "0",
        "Pane.qml re-opens the folder when hidden changes, so the cursor goes back to row 0"
    );
}
