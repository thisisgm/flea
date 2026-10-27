//! Feedback and focus contracts reached through the window's mounted IPC adapters.
use crate::{
    backend::{Backend, Event as BackendEvent},
    ipc::server::Event,
    state::{tests::shell_with, Shell},
    window::startup::View,
};
use flea::{
    ipc::wire::{Reply, Request},
    protocol::{Reply as BackendReply, RowFacts},
    ui_logic_fb1::status::Hint,
};
use gpui::{px, size, AppContext, TestAppContext, WindowHandle};

fn open(cx: &mut TestAppContext) -> (WindowHandle<Shell>, impl FnMut() -> Option<(usize, usize)>) {
    let (backend, requests) = Backend::test_channel();
    let handle = cx.open_window(size(px(1100.), px(700.)), |window, cx| {
        let mut shell = shell_with(backend, cx);
        shell.set_view(View::List, window, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    frame(cx, handle);
    (handle, requests)
}

fn frame(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn read(cx: &mut TestAppContext, handle: WindowHandle<Shell>, function: &str) -> Reply {
    handle
        .update(cx, |shell, window, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: function.into(),
                        arguments: Vec::new(),
                    },
                    answer,
                },
                window,
                cx,
            );
            receive.try_recv().unwrap().reply
        })
        .unwrap()
}

fn say(cx: &mut TestAppContext, handle: WindowHandle<Shell>, hint: Hint) {
    handle
        .update(cx, |shell, _, cx| {
            shell.status_messages(cx, |messages, now| {
                messages.say("Copied 1 item", false, "", hint, now)
            });
        })
        .unwrap();
    frame(cx, handle);
}

#[gpui::test]
fn ipc_reader_port_c_last_message_keeps_paste_and_undo_hints(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    for (hint, expected) in [
        (Hint::Paste, "Copied 1 item · p pastes"),
        (Hint::Undo, "Copied 1 item · z undoes"),
        (Hint::None, "Copied 1 item"),
        (Hint::Dismiss, "Copied 1 item"),
    ] {
        say(cx, handle, hint);
        assert_eq!(read(cx, handle, "lastMessage"), Reply::value(expected));
    }
}

#[gpui::test]
fn ipc_reader_port_c_secondary_reads_the_live_lane(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    assert_eq!(read(cx, handle, "statusSecondary"), Reply::value(""));
    for (hint, expected) in [
        (Hint::Paste, " · p pastes"),
        (Hint::Undo, " · z undoes"),
        (Hint::None, ""),
    ] {
        say(cx, handle, hint);
        assert_eq!(read(cx, handle, "statusSecondary"), Reply::value(expected));
    }
}

#[gpui::test]
fn ipc_reader_port_c_error_advice_does_not_invent_a_dismiss_suffix(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, _, cx| {
            shell.status_messages(cx, |messages, now| {
                messages.say("This location has no Trash", true, "", Hint::Deletes, now)
            });
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(
        read(cx, handle, "lastMessage"),
        Reply::value("This location has no Trash · shift-delete deletes")
    );
    assert_eq!(
        read(cx, handle, "statusSecondary"),
        Reply::value(" · esc dismisses · shift-delete deletes")
    );
}

#[gpui::test]
fn ipc_reader_port_c_end_y_includes_the_drawn_footer_extent(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.scroll_to(f32::MAX);
        })
        .unwrap();
    frame(cx, handle);
    let expected = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let drawn = host.input.drawn.expect("the real list input frame").frame;
            assert!(drawn.content > drawn.viewport());
            (drawn.content - drawn.viewport()).round().to_string()
        })
        .unwrap();
    assert_eq!(read(cx, handle, "viewEndY"), Reply::value(expected));
}

#[gpui::test]
fn ipc_reader_port_c_preview_focus_reports_the_owning_list(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            assert!(pane.begin("/files".into(), Default::default(), pane.now()));
            for reply in [
                BackendReply::Listed {
                    path: "/files".into(),
                    total: 1,
                },
                BackendReply::Rows {
                    start: 0,
                    listing: 5,
                    rows: vec![RowFacts {
                        name: "notes.txt".into(),
                        ..Default::default()
                    }],
                },
            ] {
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
            }
            shell.view = View::Columns;
            shell.sync_selection_previews(window, cx);
            let preview = shell.selection_preview(0).unwrap();
            assert!(preview.read(cx).visible);
            shell.key(
                &gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse("ctrl-tab").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            );
            assert!(preview.read(cx).focus.is_focused(window));
        })
        .unwrap();
    assert_eq!(read(cx, handle, "focusView"), Reply::value("list"));
}

#[gpui::test]
fn ipc_reader_port_c_root_mounts_every_assigned_reader(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    for name in ["lastMessage", "statusSecondary", "viewEndY", "focusView"] {
        let reply = read(cx, handle, name);
        assert_eq!(reply.code, 0, "{name}: {}", reply.stderr);
    }
}
