//! Selection opening through the production Shell, including marks outside its held window.
use super::opener_tests::{launched, looking, quit, shell};
use super::test_shell::{land_on, row, set_cursor, status};
use crate::backend::{Backend, Event};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn receive(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Event::Producer(line.into()), window, cx)
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn gh350_enter_resolves_offscreen_marks_before_one_ordered_handoff(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    cx.simulate_keystrokes("f5");
    let count = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.first_count()
    });
    let rows = (0..count)
        .map(|index| {
            row(
                &match index {
                    0 => "one.png".into(),
                    1 => "two.png".into(),
                    _ => format!("filler-{index}.txt"),
                },
                false,
            )
        })
        .collect();
    land_on(&shell, cx, 0, "/files", rows, 1000, 2);
    assert!(
        shell.read_with(cx, |shell, _| {
            let held = &shell.panes[0].as_ref().unwrap().pane.held;
            held.row(900).is_none() && held.row(901).is_none()
        }),
        "the marked tail must really be outside the held window"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .held
            .listing),
        2,
        "the fresh listing must have landed"
    );
    assert!(!shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.busy()));
    set_cursor(&shell, cx, 0);
    let (backend, mut requests) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 1, 900, 901]
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        launched().is_empty(),
        "Enter must resolve every marked row before launching, including offscreen rows"
    );
    let id = shell.read_with(cx, |shell, _| shell.menu.sequence);
    let snapshot = requests().expect("Enter requests the backend snapshot");
    assert!(
        snapshot.contains(r#""rows":[0,1,900,901]"#) && snapshot.contains(r#""listing":2"#),
        "all selected row indices and their generation: {snapshot}"
    );
    receive(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"count":4,"kinds":[32768,32768,32768,32768]}}"#
        ),
    );
    assert!(
        launched().is_empty(),
        "snapshot alone cannot launch before validation"
    );
    assert!(requests().unwrap().contains(r#""op":"validate""#));
    let reply = format!(
        r#"{{"t":"menuaction","op":"validate","action":"open","id":{id},"ok":true,"paths":["/files/one.png","/files/two.png","/files/three.png","/files/notes.txt"]}}"#
    );
    receive(&shell, cx, &reply);
    assert_eq!(
        launched(),
        ["/files/one.png /files/two.png /files/three.png /files/notes.txt"]
    );
    receive(&shell, cx, &reply);
    assert!(
        launched().is_empty(),
        "duplicate replies cannot launch twice"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_marked_folder_with_files_refuses_without_navigating(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    set_cursor(&shell, cx, 0);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 1]
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(
        super::test_shell::asked(&shell, cx),
        "/files",
        "a mixed batch must not enter its cursor folder"
    );
    let id = shell.read_with(cx, |shell, _| shell.menu.sequence);
    receive(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"count":2,"kinds":[16877,33188]}}"#
        ),
    );
    assert!(launched().is_empty());
    assert_eq!(
        status(&shell, cx),
        "A folder can’t be opened together with files."
    );
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_changed_selection_discards_a_late_snapshot(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![1, 3]
    });
    cx.simulate_keystrokes("enter");
    let id = shell.read_with(cx, |shell, _| shell.menu.sequence);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![1]
    });
    receive(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"kinds":[32768,32768]}}"#
        ),
    );
    assert!(
        launched().is_empty(),
        "the captured indices no longer name the selection"
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        launched(),
        ["/files/notes.txt"],
        "the abandoned resolution releases the single-flight lane"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_incomplete_snapshot_refuses_instead_of_opening_a_subset(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![1, 3]
    });
    cx.simulate_keystrokes("enter");
    let id = shell.read_with(cx, |shell, _| shell.menu.sequence);
    receive(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"kinds":[32768]}}"#),
    );
    assert!(launched().is_empty());
    assert_eq!(status(&shell, cx), "Some marked files haven’t loaded yet.");
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_enter_with_one_marked_cursor_folder_keeps_single_folder_navigation(
    cx: &mut TestAppContext,
) {
    let (shell, cx, _, _keep) = shell(cx);
    set_cursor(&shell, cx, 0);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0]
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(super::test_shell::asked(&shell, cx), "/files/alpha");
    assert!(launched().is_empty());
    quit(&shell, cx);
}

fn resolve_archive(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) {
    let id = shell.read_with(cx, |shell, _| shell.menu.sequence);
    receive(
        shell,
        cx,
        &format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"kinds":[32768]}}"#),
    );
    receive(
        shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"validate","action":"open","id":{id},"ok":true,"paths":["/files/backup.tar.zst"]}}"#
        ),
    );
}

#[gpui::test]
fn gh350_one_marked_archive_uses_quick_look_for_its_validated_row(cx: &mut TestAppContext) {
    let (shell, cx, (actions, _), _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![2]
    });
    cx.simulate_keystrokes("enter");
    resolve_archive(&shell, cx);
    assert!(
        launched().is_empty(),
        "a marked archive must never reach an external handler"
    );
    // The preview family owns the key, so the archive opens Quick Look instead of handing preview on.
    assert!(actions.borrow().is_empty(), "{:?}", actions.borrow());
    assert_eq!(
        looking(&shell, cx),
        "true",
        "the marked archive opens in Quick Look"
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
        2
    );
    quit(&shell, cx);
}

fn offscreen_archive(cx: &mut TestAppContext, session: bool) {
    let (shell, cx, (actions, _), _keep) = shell(cx);
    cx.simulate_keystrokes("f5");
    let count = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.first_count()
    });
    land_on(
        &shell,
        cx,
        0,
        "/files",
        (0..count)
            .map(|i| row(&format!("filler-{i}.txt"), false))
            .collect(),
        1000,
        2,
    );
    assert!(shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        !host.pane.busy() && host.pane.held.listing == 2 && host.pane.held.row(900).is_none()
    }));
    let (backend, mut windows) = Backend::test_channel();
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.backend = Some(backend);
        host.selected = vec![900];
        if session {
            host.pane.swap.session.open = true;
            host.pane.swap.session.generation = 3;
            host.pane.swap.session.opened_generation = 2;
        }
    });
    cx.simulate_keystrokes("enter");
    resolve_archive(&shell, cx);
    assert!(
        launched().is_empty(),
        "the offscreen archive must stay inside Flea"
    );
    assert!(
        actions.borrow().is_empty(),
        "Quick Look waits for its own row"
    );
    assert_eq!(
        looking(&shell, cx),
        "false",
        "Quick Look waits for its own row"
    );
    let (start, count) =
        windows().expect("the existing bounded window request loads the archive row");
    assert!(start <= 900 && start + count > 900 && count < 1000);
    let rows = (start..(start + count).min(1000))
        .map(|i| {
            row(
                if i == 900 {
                    "backup.tar.zst"
                } else {
                    "filler.txt"
                },
                false,
            )
        })
        .collect();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                if session {
                    Event::Session {
                        generation: 3,
                        reply: flea::protocol::Reply::Rows {
                            start,
                            rows,
                            listing: 2,
                        },
                    }
                } else {
                    Event::Reply(flea::protocol::Reply::Rows {
                        start,
                        rows,
                        listing: 2,
                    })
                },
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
    // The preview family owns the key, so the archive opens Quick Look instead of handing preview on.
    assert!(actions.borrow().is_empty(), "{:?}", actions.borrow());
    assert_eq!(
        looking(&shell, cx),
        "true",
        "the marked archive opens in Quick Look"
    );
    assert!(launched().is_empty());
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .held
            .row(900)
            .unwrap()
            .name
            .clone()),
        "backup.tar.zst"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_one_offscreen_marked_archive_loads_its_row_before_quick_look(cx: &mut TestAppContext) {
    offscreen_archive(cx, false);
}

#[gpui::test]
fn gh350_offscreen_marked_archive_accepts_its_active_search_session_rows(cx: &mut TestAppContext) {
    offscreen_archive(cx, true);
}
