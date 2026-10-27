//! Reader contracts exercised through the same Root dispatch as socket requests.
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use flea::ipc::wire::{Reply, Request};
use gpui::{AppContext, Context, TestAppContext, Window, WindowHandle};

const NAMES: [&str; 19] = [
    "archiveCardsState",
    "archiveFormats",
    "convertFormat",
    "convertOpen",
    "convertStrip",
    "dialogOpen",
    "flashState",
    "menuDialogState",
    "openWithState",
    "permissionsState",
    "runCardState",
    "unlockState",
    "canConvert",
    "convertState",
    "shareBrowserEntries",
    "shareBrowserOpen",
    "contextMenuFocusState",
    "statusBusy",
    "statusSecondary",
];

fn open(cx: &mut TestAppContext) -> (WindowHandle<Root>, impl FnMut() -> Option<(usize, usize)>) {
    let (backend, receive) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    (handle, receive)
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

fn read(cx: &mut TestAppContext, handle: WindowHandle<Root>, name: &str) -> Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    update(cx, handle, |shell, window, cx| {
        shell.ipc(
            super::server::Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: name.into(),
                    arguments: vec![],
                },
                answer,
            },
            window,
            cx,
        )
    });
    receive.try_recv().unwrap().reply
}

fn value(cx: &mut TestAppContext, handle: WindowHandle<Root>, name: &str, expected: &str) {
    assert_eq!(read(cx, handle, name), Reply::value(expected), "{name}");
}

fn blocked(cx: &mut TestAppContext, handle: WindowHandle<Root>, name: &str) {
    let reply = read(cx, handle, name);
    assert_ne!(reply.code, 0, "an unavailable field must not be fabricated");
    assert!(
        reply
            .stderr
            .starts_with(&format!("FLEA_IPC_ERROR flea.{name} ")),
        "{reply:?}"
    );
}

#[gpui::test]
fn ipc_reader_port_a_root_mounts_all_assigned_names(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    for view in [
        None,
        Some(crate::window::startup::View::List),
        Some(crate::window::startup::View::Grid),
        Some(crate::window::startup::View::Columns),
        Some(crate::window::startup::View::Dual),
    ] {
        update(cx, handle, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().drawn_view = view
        });
        for name in NAMES {
            let reply = read(cx, handle, name);
            assert!(
                !reply.stderr.contains("FLEA_IPC_NOT_PORTED"),
                "{view:?} {name}: {reply:?}"
            );
        }
    }
}

macro_rules! closed_reader {
    ($test:ident, $name:literal, $expected:literal) => {
        #[gpui::test]
        fn $test(cx: &mut TestAppContext) {
            let (handle, _backend) = open(cx);
            value(cx, handle, $name, $expected);
        }
    };
}

closed_reader!(ipc_archive_formats_unreported, "archiveFormats", "");
closed_reader!(ipc_convert_format_unbuilt, "convertFormat", "");
closed_reader!(ipc_convert_open_unbuilt, "convertOpen", "false");
closed_reader!(ipc_convert_strip_unbuilt, "convertStrip", "false");
closed_reader!(ipc_network_dialog_unbuilt, "dialogOpen", "false");
closed_reader!(ipc_unlock_unbuilt, "unlockState", "{\"opened\":false}");
closed_reader!(ipc_convert_capability_unreported, "canConvert", "false");
closed_reader!(
    ipc_convert_state_unbuilt,
    "convertState",
    "{\"opened\":false}"
);
closed_reader!(ipc_share_entries_unbuilt, "shareBrowserEntries", "");
closed_reader!(ipc_share_open_unbuilt, "shareBrowserOpen", "false");

macro_rules! unavailable_reader {
    ($test:ident, $name:literal) => {
        #[gpui::test]
        fn $test(cx: &mut TestAppContext) {
            let (handle, _backend) = open(cx);
            blocked(cx, handle, $name);
        }
    };
}

unavailable_reader!(ipc_menu_dialog_requires_a_mounted_owner, "menuDialogState");
unavailable_reader!(ipc_open_with_requires_a_mounted_owner, "openWithState");
unavailable_reader!(ipc_permissions_requires_a_mounted_owner, "permissionsState");
unavailable_reader!(ipc_run_card_requires_a_mounted_owner, "runCardState");
unavailable_reader!(
    ipc_menu_focus_requires_an_oracle_active_item_contract,
    "contextMenuFocusState"
);
unavailable_reader!(ipc_flash_closed_requires_a_built_lifetime, "flashState");
unavailable_reader!(
    ipc_archive_cards_require_retained_lifetime_and_eye_bounds,
    "archiveCardsState"
);

#[path = "reader_port_a_live_tests.rs"]
mod live;

#[gpui::test]
fn ipc_status_busy_reads_live_footer(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    update(cx, handle, |shell, _, cx| {
        shell.footer(cx);
    });
    value(cx, handle, "statusBusy", "false");
    update(cx, handle, |shell, _, cx| {
        shell.status_messages(cx, |messages, now| {
            messages.set_activity(0, "Starting", 8, true, true, now)
        });
    });
    value(cx, handle, "statusBusy", "true");
    update(cx, handle, |shell, _, cx| {
        shell.status_messages(cx, |messages, now| {
            messages.set_activity(0, "Copying", 8, true, false, now)
        });
    });
    value(cx, handle, "statusBusy", "false");
}

#[gpui::test]
fn ipc_status_secondary_reads_live_footer(cx: &mut TestAppContext) {
    let (handle, _backend) = open(cx);
    update(cx, handle, |shell, _, cx| {
        shell.footer(cx);
    });
    value(cx, handle, "statusSecondary", "");
}
