//! Recent leaves through the real tab strip and sorts without changing folder preferences.
use super::{
    railrecent_tests::{frame, read, rows},
    test_shell::{land, open_root_with, row, sent},
};
use crate::{ipc::Adapter, state::Shell, tabs::bar::Hit, window::navigate::Step};
use flea::{jsondoc::Json, ui_logic_rail_places::recent::Visit};
use gpui::{
    point, px, Context, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, Window,
};

fn act(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>),
) {
    cx.update(|window, cx| shell.update(cx, |shell, cx| f(shell, window, cx)));
    frame(cx);
}

fn pair(shell: &Entity<Shell>, cx: &mut VisualTestContext, path: &str, recent: bool) {
    let listing = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.greatest_listing + 1
    });
    let names = if recent {
        vec![row("files/notes.txt", false), row("files/readme.md", false)]
    } else {
        vec![row("A", true), row("notes.txt", false)]
    };
    land(shell, cx, path, names, listing);
    frame(cx);
}

fn setup<'a>(
    cx: &'a mut TestAppContext,
    origin: &str,
    count: usize,
    remember: bool,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let settings = format!(
        r#""keys":"default","view":"list","newTab":"current","rememberSort":{remember},"sort":{{"key":"name","reverse":false}},"folderSorts":{{"/":{{"key":"size","reverse":true}}}}"#
    );
    let (shell, cx) = open_root_with(cx, &settings, None);
    if origin != "/files" {
        act(&shell, cx, |shell, window, cx| {
            assert_eq!(
                shell.step(0, Step::Open(origin.into()), window, cx),
                Ok(true)
            );
        });
    }
    pair(&shell, cx, origin, false);
    for _ in 1..count {
        cx.simulate_keystrokes("t");
        frame(cx);
    }
    assert_eq!(read(&shell, cx, "tabCount"), count.to_string());
    rows(
        &shell,
        cx,
        vec![flea::ui_logic_rail_places::order::recent_entry()],
    );
    (shell, cx)
}

fn recent(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    act(shell, cx, |shell, window, cx| {
        shell.recent_open(
            0,
            vec![
                Visit {
                    path: "/files/notes.txt".into(),
                    stamp: "2026-10-30T09:00:00Z".into(),
                },
                Visit {
                    path: "/files/readme.md".into(),
                    stamp: "2026-10-29T09:00:00Z".into(),
                },
            ],
            window,
            cx,
        );
    });
    pair(shell, cx, "/", true);
    assert_eq!(read(shell, cx, "recentMode"), "results");
    assert_eq!(read(shell, cx, "sortMark"), "mtime:desc");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.path_labels[0].to_string()),
        "Recent"
    );
    sent();
}

fn click(shell: &Entity<Shell>, cx: &mut VisualTestContext, target: Hit) {
    frame(cx);
    let at = shell.update(cx, |shell, _| {
        let bounds = shell.strip.bounds.get().expect("the strip is painted");
        let at = match target {
            Hit::New => {
                let frame = &shell
                    .strip
                    .snapshot
                    .as_ref()
                    .expect("a completed strip")
                    .frame;
                let x = crate::tabs::bar::PAD_X
                    + frame.labels.len() as f32 * frame.tab_width
                    + crate::tabs::bar::HIT_MIN / 2.;
                point(
                    bounds.left() + px(x),
                    bounds.top() + px(crate::tabs::bar::HEIGHT / 2.),
                )
            }
            Hit::Tab(index) => {
                let home = shell.locale.catalog.tr("tabs.home");
                let reply = shell
                    .tab_adapter(&home)
                    .call("tabCentre", &[index.to_string()])
                    .unwrap();
                let xy: Vec<f32> = reply
                    .stdout
                    .split_whitespace()
                    .map(|word| word.parse().unwrap())
                    .collect();
                assert_eq!(xy.len(), 2, "tabCentre names a painted target");
                point(px(xy[0]), px(xy[1]))
            }
            _ => panic!("this fixture clicks a tab or the painted plus"),
        };
        assert_eq!(shell.tab_hit(at), target, "the painted target must resolve");
        at
    });
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    frame(cx);
}

fn folder_request(path: &str) {
    assert!(
        sent().iter().any(|line| {
            let data = flea::jsondoc::parse(line).unwrap();
            data.get("c").and_then(Json::as_str) == Some("list")
                && data.get("path").and_then(Json::as_str) == Some(path)
        }),
        "leaving Recent must request the real folder {path}"
    );
}

fn left_recent(shell: &Entity<Shell>, cx: &mut VisualTestContext, path: &str) {
    folder_request(path);
    assert_eq!(read(shell, cx, "recentMode"), "");
    assert_eq!(read(shell, cx, "recentFrom"), "");
    pair(shell, cx, path, false);
    assert_eq!(read(shell, cx, "path"), path);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.recent.paths.is_empty());
        assert!(host.recent.visits.is_empty());
        assert!(host.recent.stashed.is_none());
        assert!(host.list.borrow().view.place_recent.is_none());
        assert_eq!(shell.path_labels[0].as_ref(), path);
        assert_eq!(host.pane.held.rows[0].name, "A");
    });
}

#[gpui::test]
fn sol_recent_tabs_remainder_new_tab_drops_recent_and_uses_its_resting_folder(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = setup(cx, "/files", 2, true);
    recent(&shell, cx);
    click(&shell, cx, Hit::New);
    assert_eq!(read(&shell, cx, "tabCount"), "3");
    assert_eq!(read(&shell, cx, "tabIndex"), "2");
    left_recent(&shell, cx, "/files");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn sol_recent_tabs_remainder_new_tab_relists_when_recent_and_target_both_stand_on_root(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = setup(cx, "/", 2, true);
    recent(&shell, cx);
    click(&shell, cx, Hit::New);
    assert_eq!(read(&shell, cx, "tabCount"), "3");
    left_recent(&shell, cx, "/");
    assert_eq!(read(&shell, cx, "sortMark"), "size:desc");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn sol_recent_tabs_remainder_switch_drops_recent_and_return_restores_the_source_folder(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = setup(cx, "/files", 2, true);
    recent(&shell, cx);
    click(&shell, cx, Hit::Tab(0));
    assert_eq!(read(&shell, cx, "tabIndex"), "0");
    left_recent(&shell, cx, "/files");
    click(&shell, cx, Hit::Tab(1));
    assert_eq!(read(&shell, cx, "path"), "/files");
    assert_eq!(read(&shell, cx, "recentMode"), "");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn sol_recent_tabs_remainder_refused_add_current_tab_and_hidden_close_keep_recent(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = setup(cx, "/files", crate::tabs::MAX, true);
    recent(&shell, cx);
    act(&shell, cx, |shell, window, cx| {
        shell.tab_tap_hit(Hit::New, false, window, cx);
        shell.tab_tap_hit(Hit::Tab(crate::tabs::MAX - 1), false, window, cx);
        shell.tab_tap_hit(Hit::Tab(0), true, window, cx);
    });
    assert_eq!(
        read(&shell, cx, "tabCount"),
        (crate::tabs::MAX - 1).to_string()
    );
    assert_eq!(read(&shell, cx, "recentMode"), "results");
    assert_eq!(read(&shell, cx, "recentFrom"), "/files");
    assert!(!sent().iter().any(|line| line.contains("\"c\":\"list\"")));
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.list.borrow().view.place_recent.is_some());
        assert!(host.recent.stashed.is_some());
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn sol_recent_tabs_remainder_sort_keeps_root_memory_and_existing_global_fallback(
    cx: &mut TestAppContext,
) {
    for remember in [true, false] {
        let (shell, cx) = setup(cx, "/files", 2, remember);
        let before = shell.read_with(cx, |shell, _| shell.launch.settings.data.clone());
        recent(&shell, cx);
        for (keys, mark) in [("s", "kind:asc"), ("shift-s", "kind:desc")] {
            cx.simulate_keystrokes(keys);
            assert_eq!(read(&shell, cx, "sortMark"), mark);
            assert!(sent().iter().any(|line| {
                let command = flea::jsondoc::parse(line).unwrap();
                command.get("c").and_then(Json::as_str) == Some("sort")
                    && command.get("by").and_then(Json::as_str) == Some("kind")
                    && command.get("desc").and_then(Json::as_bool) == Some(keys == "shift-s")
            }));
            pair(&shell, cx, "/", true);
        }
        act(&shell, cx, |shell, window, cx| {
            assert_eq!(
                shell.step(0, Step::SortColumn("size".into()), window, cx),
                Ok(true)
            );
        });
        pair(&shell, cx, "/", true);
        assert_eq!(read(&shell, cx, "sortMark"), "size:asc");
        act(&shell, cx, |shell, window, cx| {
            assert_eq!(shell.step(0, Step::Back, window, cx), Ok(true));
        });
        left_recent(&shell, cx, "/files");
        assert_eq!(read(&shell, cx, "sortMark"), "name:asc");
        let global_writes = if remember { 0 } else { 3 };
        shell.read_with(cx, |shell, _| {
            assert_eq!(
                shell.launch.settings.data.get("folderSorts"),
                before.get("folderSorts")
            );
            if remember {
                assert_eq!(shell.launch.settings.data.get("sort"), before.get("sort"));
            } else {
                // PaneSwap.qml:224-228 keeps a completed sort as the global fallback with memory off.
                let global = shell.launch.settings.data.get("sort").unwrap();
                assert_eq!(global.get("key").and_then(Json::as_str), Some("size"));
                assert_eq!(global.get("reverse").and_then(Json::as_bool), Some(false));
            }
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.nav.folders.borrow().get("/").unwrap().key, "size");
            let writer = host.nav.writer.borrow();
            assert_eq!(writer.sort_writes(), global_writes);
            assert!(writer.patches.iter().all(|patch| {
                flea::jsondoc::parse(patch)
                    .unwrap()
                    .get("folderSorts")
                    .is_none()
            }));
        });
        cx.simulate_keystrokes("shift-s");
        pair(&shell, cx, "/files", false);
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.nav.writer.borrow().sort_writes(), global_writes + 1);
            let folders = host.nav.folders.borrow();
            assert_eq!(folders.order_for("/files").key, "name");
            assert!(folders.order_for("/files").desc);
            assert_eq!(folders.get("/").unwrap().key, "size");
            assert!(folders.get("/").unwrap().desc);
        });
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[gpui::test]
fn sol_recent_tabs_remainder_late_recent_sort_keeps_root_memory_and_existing_fallback(
    cx: &mut TestAppContext,
) {
    for remember in [true, false] {
        let (shell, cx) = setup(cx, "/", 2, remember);
        let before = shell.read_with(cx, |shell, _| {
            shell.launch.settings.data.get("folderSorts").cloned()
        });
        recent(&shell, cx);
        cx.simulate_keystrokes("s");
        assert_eq!(read(&shell, cx, "sortMark"), "kind:asc");
        click(&shell, cx, Hit::New);
        assert_eq!(read(&shell, cx, "tabCount"), "3");
        assert_eq!(read(&shell, cx, "recentMode"), "");
        folder_request("/");
        pair(&shell, cx, "/", true);
        pair(&shell, cx, "/", false);
        // With memory on the late Recent sort writes nothing; with it off only the global fallback.
        let (global_key, writes, mark) = if remember {
            ("name", 0, "size:desc")
        } else {
            ("kind", 1, "name:asc")
        };
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                shell.launch.settings.data.get("folderSorts"),
                before.as_ref()
            );
            assert_eq!(host.nav.folders.borrow().get("/").unwrap().key, "size");
            let global = shell.launch.settings.data.get("sort").unwrap();
            assert_eq!(global.get("key").and_then(Json::as_str), Some(global_key));
            assert_eq!(global.get("reverse").and_then(Json::as_bool), Some(false));
            assert_eq!(host.nav.writer.borrow().sort_writes(), writes);
            assert_eq!(host.pane.held.rows[0].name, "A");
        });
        assert_eq!(read(&shell, cx, "sortMark"), mark);
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}
