//! The live Shell must mount Grid and route the same input and read-only IPC as the native window.
use crate::{
    backend::Backend,
    ipc::server::Event,
    state::{tests::shell_with, Shell},
    window::startup::View,
};
use flea::ipc::wire::{Reply, Request};
use gpui::{AppContext, Context, KeyDownEvent, Keystroke, TestAppContext, WindowHandle};

#[path = "pb_path_anchor_tests.rs"]
mod pb_path_anchor_tests;

pub(super) fn named_shell(backend: Backend, cx: &mut Context<Shell>) -> Shell {
    let mut shell = shell_with(backend, cx);
    let held = &mut shell.panes[0].as_mut().unwrap().pane.held;
    for (index, row) in std::rc::Rc::get_mut(held)
        .unwrap()
        .rows
        .iter_mut()
        .enumerate()
    {
        row.name = format!("file-{index}.txt");
        row.icon = "text-plain".into();
        row.mode = 0o100644;
    }
    shell
}

// The request receiver stays with the caller, so a window send reaches a live backend.
pub(super) fn open(
    cx: &mut TestAppContext,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<(usize, usize)>) {
    let (backend, requests) = Backend::test_channel();
    (cx.add_window(|_, cx| named_shell(backend, cx)), requests)
}

pub(super) fn frame(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

pub(super) fn call(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    name: &str,
    arguments: &[&str],
) -> Reply {
    handle
        .update(cx, |shell, window, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: name.into(),
                        arguments: arguments.iter().map(|s| (*s).into()).collect(),
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

pub(super) fn key(cx: &mut TestAppContext, handle: WindowHandle<Shell>, name: &str) {
    handle
        .update(cx, |shell, window, cx| {
            shell.key(
                &KeyDownEvent {
                    keystroke: Keystroke::parse(name).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            );
        })
        .unwrap();
    frame(cx, handle);
}

#[gpui::test]
fn u10_live_grid_reader_tracks_drawn_columns_and_hidden_grid_uses_list_stride(
    cx: &mut TestAppContext,
) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    let reply = call(cx, handle, "gridColumns", &[]);
    assert_eq!(reply.code, 0, "Grid must be registered in the live window");
    let columns: usize = reply.stdout.trim().parse().unwrap();
    assert!(columns > 1);
    key(cx, handle, "down");
    assert_eq!(
        call(cx, handle, "cursor", &[]),
        Reply::value(columns.to_string())
    );
    key(cx, handle, "right");
    assert_eq!(
        call(cx, handle, "cursor", &[]),
        Reply::value((columns + 1).to_string())
    );
    key(cx, handle, "up");
    assert_eq!(call(cx, handle, "cursor", &[]), Reply::value("1"));
    key(cx, handle, "left");
    key(cx, handle, "left");
    assert_eq!(
        call(cx, handle, "cursor", &[]),
        Reply::value("0"),
        "left edge consumes the key"
    );
    handle
        .update(cx, |shell, _, _| shell.keyboard.set_composing(true))
        .unwrap();
    key(cx, handle, "right");
    assert_eq!(
        call(cx, handle, "cursor", &[]),
        Reply::value("0"),
        "composition owns sideways keys too"
    );
    handle
        .update(cx, |shell, _, _| shell.keyboard.set_composing(false))
        .unwrap();
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::List, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(call(cx, handle, "gridColumns", &[]), Reply::value("1"));
    key(cx, handle, "down");
    assert_eq!(call(cx, handle, "cursor", &[]), Reply::value("1"));
}

#[gpui::test]
fn u10_live_grid_caption_and_rect_exist_only_for_painted_tiles(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(call(cx, handle, "gridCaptionState", &["0"]).code, 0);
    let rect = call(cx, handle, "rowRect", &["0"]);
    assert_eq!(rect.code, 0, "row adapter must read the drawn Grid");
    assert_eq!(rect.stdout.split_whitespace().count(), 4);
    assert_eq!(
        call(cx, handle, "gridCaptionState", &["999"]),
        Reply::value("{}")
    );
    assert_eq!(call(cx, handle, "rowRect", &["999"]), Reply::value(""));
    assert_eq!(call(cx, handle, "rowRect", &["-1"]), Reply::value(""));
    assert_eq!(
        call(cx, handle, "thumbnailPolicyState", &[]).code,
        0,
        "the thumbnail policy reader answers beside the drawn Grid"
    );
}
