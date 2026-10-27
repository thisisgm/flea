//! FIX-READERS3 (EY-228): readers v0.3.12 answers that GPUI used to refuse, reached through the production Root dispatch.
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use flea::{
    ipc::wire::{Reply, Request},
    jsondoc::{self, Json},
};
use gpui::{AppContext, Context, TestAppContext, Window, WindowHandle};

fn open(cx: &mut TestAppContext) -> WindowHandle<Root> {
    let (backend, _receive) = Backend::test_channel();
    cx.add_window(|_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    })
}

fn update<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| change(shell, window, cx))
        })
        .unwrap()
}

fn read(cx: &mut TestAppContext, handle: WindowHandle<Root>, name: &str, args: &[&str]) -> Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    let arguments = args.iter().map(|argument| (*argument).to_owned()).collect();
    update(cx, handle, |shell, window, cx| {
        shell.ipc(
            super::server::Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: name.into(),
                    arguments,
                },
                answer,
            },
            window,
            cx,
        )
    });
    receive.try_recv().unwrap().reply
}

fn value(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    name: &str,
    args: &[&str],
    expected: &str,
) {
    assert_eq!(
        read(cx, handle, name, args),
        Reply::value(expected),
        "{name}"
    );
}

fn json(cx: &mut TestAppContext, handle: WindowHandle<Root>, name: &str) -> Json {
    let reply = read(cx, handle, name, &[]);
    assert_eq!(reply.code, 0, "{name}: {reply:?}");
    jsondoc::parse(reply.stdout.trim()).unwrap()
}

#[gpui::test]
fn ipc_readers3_mode_is_always_browse(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "mode", &[], "browse");
}

#[gpui::test]
fn ipc_readers3_recent_mode_is_off_without_recent(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "recentMode", &[], "");
}

#[gpui::test]
fn ipc_readers3_recent_from_is_idle_without_recent(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "recentFrom", &[], "");
}

#[gpui::test]
fn ipc_readers3_favourites_are_not_saving_at_rest(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "favouritesSaving", &[], "false");
}

#[gpui::test]
fn ipc_readers3_menu_dialog_state_is_closed_without_a_card(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "menuDialogState", &[], "{\"opened\":false}");
}

#[gpui::test]
fn ipc_readers3_open_with_state_is_closed_without_a_card(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "openWithState", &[], "{\"opened\":false}");
}

#[gpui::test]
fn ipc_readers3_run_card_state_is_closed_without_a_card(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "runCardState", &[], "{\"opened\":false}");
}

#[gpui::test]
fn ipc_readers3_archive_cards_state_is_closed_without_a_card(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "archiveCardsState", &[], "{\"opened\":false}");
}

#[gpui::test]
fn ipc_readers3_permissions_closed_shape_keeps_input_ready(cx: &mut TestAppContext) {
    let handle = open(cx);
    let state = json(cx, handle, "permissionsState");
    assert_eq!(state.get("opened").and_then(Json::as_bool), Some(false));
    assert!(state.get("inputReady").and_then(Json::as_bool).is_some());
}

#[gpui::test]
fn ipc_readers3_row_thumb_ready_is_false_without_a_row(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "rowThumbReady", &["0"], "false");
}

#[gpui::test]
fn ipc_readers3_listing_drop_is_inactive_without_a_drag(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "listingDropActive", &[], "false");
}

#[gpui::test]
fn ipc_readers3_rail_mark_is_never_hovered_by_idle_gpui(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "railMarkHovered", &["0"], "false");
}

#[gpui::test]
fn ipc_readers3_context_menu_focus_closed_shape(cx: &mut TestAppContext) {
    let handle = open(cx);
    let state = json(cx, handle, "contextMenuFocusState");
    assert_eq!(state.get("opened").and_then(Json::as_bool), Some(false));
    assert_eq!(state.get("menu").and_then(Json::as_bool), Some(false));
    assert_eq!(state.get("origin").and_then(Json::as_bool), Some(false));
    assert_eq!(state.get("activeItem").and_then(Json::as_str), Some(""));
    for key in ["view", "pane", "list"] {
        assert!(
            state.get(key).is_some(),
            "{key} is part of the v0.3.12 shape"
        );
    }
}

#[gpui::test]
fn ipc_readers3_thumbnail_policy_idle_shape(cx: &mut TestAppContext) {
    let handle = open(cx);
    let state = json(cx, handle, "thumbnailPolicyState");
    for key in [
        "view",
        "mode",
        "files",
        "pending",
        "contentY",
        "cursor",
        "previewIndex",
        "previewPath",
        "previewReady",
    ] {
        assert!(
            state.get(key).is_some(),
            "{key} is part of the v0.3.12 shape"
        );
    }
    assert_eq!(state.get("previewIndex").and_then(Json::as_f64), Some(-1.));
    assert_eq!(
        state.get("previewReady").and_then(Json::as_bool),
        Some(false)
    );
}
