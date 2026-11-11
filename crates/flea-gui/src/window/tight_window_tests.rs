//! #454: the launch request and its reply guard share the configured density's window.
use crate::{
    backend::{Backend, Event},
    presentation::Fonts,
    state::{constructor_fixture, Shell},
    window::{
        locale::Snapshot,
        startup::{Arguments, Launch, Settings},
    },
};
use flea::protocol::{Reply, RowFacts};
use gpui::{px, size, TestAppContext, WindowHandle};
use std::{cell::Cell, sync::Arc};

#[path = "search_window_tests.rs"]
mod search_window;

const TOTAL: usize = 470;

#[derive(Default)]
struct Asked(Cell<usize>);
impl gpui::Global for Asked {}

fn open(
    cx: &mut TestAppContext,
    view: &str,
    density: &str,
    text: usize,
    (width, height): (f32, f32),
) -> (WindowHandle<Shell>, usize) {
    cx.update(|cx| {
        cx.set_global(Asked::default());
        cx.set_global(constructor_fixture::NoSettingsWorker);
        cx.set_global(Fonts::from_sources("", "", "sans-serif".into()));
        cx.set_global(constructor_fixture::Services {
            backend: |listing, cx| {
                cx.global::<Asked>().0.set(listing.first);
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        });
    });
    let handle = cx.open_window(size(px(width), px(height)), |window, cx| {
        let data = flea::jsondoc::parse(&format!(
            r#"{{"view":"{view}","density":"{density}","display":{{"textSize":{{"mode":{text}}}}}}}"#
        )).unwrap();
        let data = flea::uistate::patched(&flea::jsondoc::Json::Obj(Vec::new()), &data).unwrap();
        let launch = Launch::resolve(
            Settings::from_json(data),
            Arguments { path: Some("/files".into()), select: None },
            "/home/test",
        );
        let (_, events) = async_channel::bounded(1);
        let mut shell = Shell::new(launch, Snapshot {
            revision: 1,
            catalog: Arc::new(crate::test_catalogs::english()),
        }, events, window, cx);
        for host in shell.panes.iter_mut().flatten() {
            host.nav.writer = Default::default();
        }
        shell
    });
    let asked = cx.read(|cx| cx.global::<Asked>().0.get());
    (handle, asked)
}

fn rows(count: usize) -> Reply {
    Reply::Rows {
        start: 0,
        listing: 1,
        rows: (0..count)
            .map(|at| RowFacts {
                name: format!("file-{at:03}.txt"),
                ..Default::default()
            })
            .collect(),
    }
}

fn land(cx: &mut TestAppContext, handle: WindowHandle<Shell>, asked: usize) {
    handle
        .update(cx, |shell, window, cx| {
            for reply in [
                Reply::Listed {
                    path: "/files".into(),
                    total: TOTAL,
                },
                rows(asked),
            ] {
                shell.receive(0, Event::Reply(reply), window, cx);
            }
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.error.is_none(), "{:?}", host.error);
            assert_eq!(host.pane.held.total, TOTAL);
            assert!(!host.pane.busy());
        })
        .unwrap();
}

#[gpui::test]
fn tight_window_launch_uses_the_sent_count_in_every_view(cx: &mut TestAppContext) {
    for view in ["list", "columns", "grid"] {
        for text in [12, 14] {
            for dimensions in [(1100., 700.), (2560., 1440.)] {
                let (_, compact) = open(cx, view, "compact", text, dimensions);
                let (handle, asked) = open(cx, view, "tight", text, dimensions);
                assert!(
                    asked > compact,
                    "{view}, text {text}, {dimensions:?}: tight {asked}, compact {compact}"
                );
                handle
                    .update(cx, |shell, _, _| {
                        let pane = &shell.panes[0].as_ref().unwrap().pane;
                        assert_eq!(pane.row_height(), if text == 14 { 23. } else { 20. });
                        assert_eq!(
                            pane.swap.pending.as_ref().unwrap().first,
                            asked,
                            "{view}, text {text}, {dimensions:?}: requested {asked}"
                        );
                    })
                    .unwrap();
                land(cx, handle, asked);
            }
        }
    }
}

#[gpui::test]
fn tight_window_live_density_change_preserves_the_sent_count(cx: &mut TestAppContext) {
    for (before, after) in [("tight", "compact"), ("compact", "tight")] {
        let (handle, asked) = open(cx, "list", before, 14, (1100., 700.));
        handle
            .update(cx, |shell, window, cx| {
                let patch = flea::jsondoc::parse(&format!(r#"{{"density":"{after}"}}"#)).unwrap();
                let document = flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
                shell.receive_settings(
                    super::settings::store::Update::Loaded {
                        document,
                        through: 0,
                        saved: false,
                    },
                    window,
                    cx,
                );
                let pane = &shell.panes[0].as_ref().unwrap().pane;
                assert_ne!(pane.first_count(), asked);
                assert_eq!(pane.swap.pending.as_ref().unwrap().first, asked);
            })
            .unwrap();
        land(cx, handle, asked);
    }
}

#[gpui::test]
fn tight_window_still_rejects_short_and_oversized_replies(cx: &mut TestAppContext) {
    for delta in [-1, 1] {
        let (handle, asked) = open(cx, "list", "tight", 14, (1100., 700.));
        handle
            .update(cx, |shell, _, _| {
                let pane = &mut shell.panes[0].as_mut().unwrap().pane;
                pane.receive_at(
                    Reply::Listed {
                        path: "/files".into(),
                        total: TOTAL,
                    },
                    0,
                )
                .unwrap();
                assert_eq!(
                    pane.receive_at(rows(asked.checked_add_signed(delta).unwrap()), 0),
                    Err("pane.reply_window".into())
                );
                assert_eq!(pane.held.total, 0);
            })
            .unwrap();
    }
}
