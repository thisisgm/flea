//! EY-466: a Trash watch or count failure reaches the window's status bar while the rail shows the count (Sidebar.qml onFailed).
use crate::{
    backend::Backend,
    g2::pl3::monitor::Heard,
    state::{constructor_fixture::Services, Shell},
    window::{
        locale::Snapshot,
        startup::{Arguments, Launch, Settings},
    },
};
use gpui::{TestAppContext, WindowHandle};
use std::sync::Arc;

fn open(cx: &mut TestAppContext, settings: &str) -> WindowHandle<Shell> {
    cx.update(|cx| {
        cx.set_global(Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: Some(|| Err(std::io::Error::other("gio monitor exited 1"))),
        })
    });
    let launch = Launch::resolve(
        Settings::from_json(flea::jsondoc::parse(settings).unwrap()),
        Arguments {
            path: Some("/files".into()),
            select: None,
        },
        "/home/test",
    );
    let handle = cx.add_window(|window, cx| {
        let (_events_tx, events) = async_channel::bounded(1);
        let locale = Snapshot {
            revision: 1,
            catalog: Arc::new(crate::test_catalogs::english()),
        };
        Shell::new(launch, locale, events, window, cx)
    });
    cx.run_until_parked();
    handle
}

fn said(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> Vec<String> {
    handle
        .update(cx, |shell, _, cx| {
            shell.status_messages(cx, |messages, _| {
                messages
                    .errors()
                    .iter()
                    .map(|e| e.text.clone())
                    .collect::<Vec<_>>()
            })
        })
        .unwrap()
}

fn tr(cx: &mut TestAppContext, handle: WindowHandle<Shell>, key: &str) -> String {
    handle
        .update(cx, |shell, _, _| shell.locale.catalog.tr(key))
        .unwrap()
}

fn heard(cx: &mut TestAppContext, handle: WindowHandle<Shell>, key: &'static str) {
    handle
        .update(cx, |shell, _, cx| {
            let pane = shell.trash.as_ref().unwrap().pane.clone();
            pane.update(cx, |pane, cx| pane.heard(Heard::Failed(key), cx));
        })
        .unwrap();
    cx.run_until_parked();
}

#[gpui::test]
fn ey466_trash_watch_and_count_failures_reach_the_status_bar_with_the_count_shown(
    cx: &mut TestAppContext,
) {
    let handle = open(cx, r#"{"places":{"trashCount":true}}"#);
    let watch = tr(cx, handle, "trash.monitor.watch_failed");
    assert_eq!(
        said(cx, handle),
        [watch],
        "a watch that cannot start is said"
    );
    heard(cx, handle, "trash.monitor.count_failed");
    let count = tr(cx, handle, "trash.monitor.count_failed");
    assert!(
        said(cx, handle).contains(&count),
        "an unreadable count is said: {:?}",
        said(cx, handle)
    );
}

#[gpui::test]
fn ey466_trash_monitor_failures_stay_quiet_with_the_count_hidden(cx: &mut TestAppContext) {
    let handle = open(cx, "{}");
    heard(cx, handle, "trash.monitor.count_failed");
    assert!(said(cx, handle).is_empty(), "{:?}", said(cx, handle));
}
