//! Trash IPC controls are reached through the normal window's mounted adapter set.
use super::test_shell::{land, open_root, row};
use crate::{backend::Event as BackendEvent, ipc::server::Event, state::Shell};
use flea::ipc::wire::Request;
use gpui::{Entity, TestAppContext, VisualTestContext};

fn call(shell: &Entity<Shell>, cx: &mut VisualTestContext, name: &str, args: &[&str]) -> String {
    let (answer, reply) = std::sync::mpsc::sync_channel(1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: name.into(),
                        arguments: args.iter().map(|arg| (*arg).into()).collect(),
                    },
                    answer,
                },
                window,
                cx,
            );
        })
    });
    let reply = reply.try_recv().unwrap().reply;
    assert_eq!(reply.code, 0, "{name}: {reply:?}");
    reply.stdout.trim_end().into()
}

fn draw(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(400));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn trash_parity_ipc_controls_match_closed_and_open_view_contract(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    for (name, args, expected) in [
        ("trashControlCentre", vec!["empty"], ""),
        ("trashEmptyState", vec![], ""),
        ("trashFocusEmpty", vec![], "false"),
        ("trashFocusListing", vec![], "false"),
    ] {
        assert_eq!(call(&shell, cx, name, &args), expected, "{name}");
    }
    cx.update(|window, cx| shell.update(cx, |shell, cx| {
        shell.mount_trash(window, cx);
        shell.trash.as_ref().unwrap().pane.update(cx, |pane, _| pane.monitor_start = None);
        shell.trash_action("openTrash", window, cx);
        let id = shell.trash.as_ref().unwrap().pane.read(cx).view().request_id;
        shell.receive(0, BackendEvent::TrashLine(format!(r#"{{"t":"trashbrowse","id":{id},"op":"list","ok":true,"total":1,"start":0,"rows":[{{"uri":"trash:///notes.txt","original":"/files/notes.txt","identity":"notes","size":13}}]}}"#)), window, cx);
        let summary = shell.trash.as_ref().unwrap().pane.read(cx).view().request_id;
        shell.receive(0, BackendEvent::TrashLine(format!(r#"{{"t":"trashbrowse","id":{summary},"op":"summary","ok":true,"bytes":13,"partial":false}}"#)), window, cx);
    }));
    draw(cx);
    assert_eq!(
        call(&shell, cx, "trashEmptyState", &[]),
        "false|false|false|true"
    );
    assert_eq!(call(&shell, cx, "trashFocusEmpty", &[]), "true");
    assert_eq!(
        call(&shell, cx, "trashEmptyState", &[]),
        "false|true|false|true"
    );
    assert_eq!(call(&shell, cx, "trashFocusListing", &[]), "true");
    assert_eq!(
        call(&shell, cx, "trashEmptyState", &[]),
        "false|false|false|true"
    );
    for control in ["back", "up", "empty"] {
        let centre = call(&shell, cx, "trashControlCentre", &[control]);
        assert_eq!(centre.split_whitespace().count(), 2, "{control}: {centre}");
    }
    assert_eq!(call(&shell, cx, "trashControlCentre", &["unknown"]), "");
    assert_eq!(call(&shell, cx, "trashControlCentre", &["danger"]), "");
    let centre = call(&shell, cx, "trashControlCentre", &["empty"]);
    let mut values = centre
        .split_whitespace()
        .map(|value| value.parse::<f32>().unwrap());
    cx.simulate_mouse_move(
        gpui::point(
            gpui::px(values.next().unwrap()),
            gpui::px(values.next().unwrap()),
        ),
        None,
        gpui::Modifiers::none(),
    );
    draw(cx);
    assert_eq!(
        call(&shell, cx, "trashEmptyState", &[]),
        "true|false|false|true"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
#[ignore = "known red until the rail Trash row menu lands"]
fn trash_parity_rail_menu_opens_and_returns_to_the_trash_host(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    cx.simulate_keystrokes("tab down shift-f10");
    draw(cx);
    let entries = call(&shell, cx, "contextMenuEntries", &[]);
    assert!(
        entries.contains("Restore All"),
        "the Trash rail menu is drawn: {entries}"
    );
    assert!(entries.contains("Empty Trash"), "{entries}");
    cx.simulate_keystrokes("enter");
    assert!(shell.read_with(cx, |shell, cx| shell.trash_opened(cx)));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn trash_parity_confirmation_buttons_come_from_the_drawn_delete_card(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.mount_trash(window, cx);
            shell.trash.as_ref().unwrap().pane.update(cx, |pane, _| pane.monitor_start = None);
            shell.trash_action("openTrash", window, cx);
            let id = shell.trash.as_ref().unwrap().pane.read(cx).view().request_id;
            shell.receive(0, BackendEvent::TrashLine(format!(r#"{{"t":"trashbrowse","id":{id},"op":"list","ok":true,"total":1,"start":0,"rows":[{{"uri":"trash:///notes.txt","original":"/files/notes.txt","identity":"notes","size":13}}]}}"#)), window, cx);
            let summary = shell.trash.as_ref().unwrap().pane.read(cx).view().request_id;
            shell.receive(0, BackendEvent::TrashLine(format!(r#"{{"t":"trashbrowse","id":{summary},"op":"summary","ok":true,"bytes":13,"partial":false}}"#)), window, cx);
        })
    });
    draw(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let pane = shell.trash.as_ref().unwrap().pane.clone();
            pane.update(cx, |pane, cx| pane.empty_pressed(cx));
            let prepare = pane.read(cx).view().request_id;
            shell.receive(0, BackendEvent::TrashLine(format!(r#"{{"t":"trashbrowse","id":{prepare},"op":"prepare","ok":true,"token":5,"count":1}}"#)), window, cx);
        })
    });
    draw(cx);
    for button in ["cancel", "danger"] {
        let centre = call(&shell, cx, "trashControlCentre", &[button]);
        assert_eq!(centre.split_whitespace().count(), 2, "{button}: {centre:?}");
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
