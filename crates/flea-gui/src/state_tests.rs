use super::*;
use crate::window::startup::{Arguments, Settings};
use crate::{backend::Event as BackendEvent, list::pane::ROW_HEIGHT};
use flea::protocol::Reply;
use gpui::{px, size, TestAppContext};
use gpui::{AppContext, Render};
use std::rc::Rc;
use std::sync::Arc;

#[path = "../tests/g1_04/host.rs"]
mod g1_04;
#[path = "../tests/g1_12/host.rs"]
mod g1_12;

#[path = "state_test_fixture.rs"]
mod fixture;
pub(crate) use fixture::shell_with;

#[gpui::test]
fn adv_e_r1_dead_backend_prints_once_across_many_renders(cx: &mut TestAppContext) {
    let (backend, receiver) = Backend::test_channel();
    drop(receiver);
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    for wanted in 2..66 {
        cx.simulate_resize(size(px(640.), px((wanted + 1) as f32 * ROW_HEIGHT)));
        cx.update(|window, cx| {
            entity.update(cx, |shell, cx| {
                shell.panes[0]
                    .as_mut()
                    .unwrap()
                    .pane
                    .scroll_to(600. * ROW_HEIGHT);
                let _ = shell.render(window, cx);
                assert!(shell.panes[0].as_ref().unwrap().stopped);
                assert!(shell.panes[0].as_ref().unwrap().error.is_some());
            })
        });
    }
}

#[gpui::test]
fn adv_e_r2_unrelated_replies_keep_the_window_request_in_flight(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let entity = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.scroll_to(600. * ROW_HEIGHT);
            let original = Rc::clone(&host.pane.held);
            let _ = shell.render(window, cx);
            assert!(requests().is_some());
            for reply in [
                Reply::Other,
                Reply::Listed {
                    path: "/elsewhere".into(),
                    total: 99,
                },
                Reply::Rows {
                    start: 0,
                    rows: Vec::new(),
                    listing: 1,
                },
                Reply::Error {
                    operation: "window".into(),
                    path: "/elsewhere".into(),
                    message: "unrelated".into(),
                    mode: 0,
                },
            ] {
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
                let _ = shell.render(window, cx);
                assert_eq!(requests(), None);
                assert!(Rc::ptr_eq(
                    &original,
                    &shell.panes[0].as_ref().unwrap().pane.held
                ));
            }
        })
    });
}

#[gpui::test]
fn ipc_executor_reads_the_current_model_without_starting_io(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 4;
            shell.panes[0].as_mut().unwrap().selected = vec![2, 4];
            for (name, expected) in [
                ("total", "1000"),
                ("cursor", "4"),
                ("selectedIndices", "2,4"),
                ("path", "/files"),
            ] {
                let (answer, receive) = std::sync::mpsc::sync_channel(1);
                shell.ipc(
                    Event::Call {
                        request: flea::ipc::wire::Request::Call {
                            target: "flea".into(),
                            function: name.into(),
                            arguments: vec![],
                        },
                        answer,
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    receive.try_recv().unwrap().reply,
                    flea::ipc::wire::Reply::value(expected)
                );
            }
        });
    });
}

#[gpui::test]
fn language_change_invalidates_shaped_rows_and_refreshes_snapshot_text(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_channel();
    let entity = cx.new(|cx| shell_with(backend, cx));
    entity.update(cx, |shell, cx| {
        shell.panes[0].as_mut().unwrap().answered = true;
        {
            use crate::row::cache::{Prepared, Shaping};
            use crate::row::{marks::Mark, NameInk};
            let host = shell.panes[0].as_ref().unwrap();
            let mut list = host.list.borrow_mut();
            list.rows.begin(
                &Shaping {
                    listing: 2,
                    font: gpui::font("JetBrains Mono"),
                    body: px(14.),
                    caption: px(12.),
                    name_cells: 40,
                    scale: 1.,
                    ..Shaping::default()
                },
                3,
            );
            list.rows
                .get_or_prepare(0, &flea::protocol::RowFacts::default(), || {
                    let layout = Arc::new(gpui::LineLayout::default());
                    Prepared {
                        label: String::new(),
                        size_text: String::new(),
                        date_text: String::new(),
                        name: gpui::GlyphRun::new(layout.clone()),
                        size: gpui::GlyphRun::new(layout.clone()),
                        date: gpui::GlyphRun::new(layout),
                        mark: Mark::File,
                        ink: NameInk::Foreground,
                        ..Prepared::default()
                    }
                });
            assert_eq!(list.rows.len(), 1);
        }
        let snapshot = Snapshot {
            revision: 2,
            catalog: shell.locale.catalog.clone(),
        };
        assert_eq!(requests(), None);
        shell.apply_locale(snapshot, cx);
        assert_eq!(shell.locale.revision, 2);
        assert_eq!(requests(), Some((0, 320)));
        assert_eq!(requests(), None);
        assert_eq!(
            shell.panes[0].as_ref().unwrap().status.as_ref(),
            "1,000 items"
        );
        assert_eq!(shell.panes[0].as_ref().unwrap().list.borrow().rows.len(), 0);
    });
}

#[gpui::test]
fn dual_is_constructed_on_first_demand_and_retained_without_hidden_rendering(
    cx: &mut TestAppContext,
) {
    let (backend, _) = Backend::test_channel();
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        let entity = cx.new(|cx| shell_with(backend, cx));
        entity.update(cx, |shell, cx| {
            assert!(shell.panes[1].is_none());
            assert_eq!(shell.lifecycle.live_count(), 1);
            for view in [View::Columns, View::Grid, View::List] {
                shell.set_view(view, window, cx);
                assert!(shell.panes[1].is_none());
            }
            shell.set_view(View::Dual, window, cx);
            assert_eq!(shell.lifecycle.live_count(), 2);
            let second = Rc::clone(&shell.panes[1].as_ref().unwrap().list);
            shell.set_view(View::List, window, cx);
            let _ = shell.render(window, cx);
            assert_eq!(second.borrow().rows.misses, 0);
            shell.set_view(View::Dual, window, cx);
            assert!(Rc::ptr_eq(&second, &shell.panes[1].as_ref().unwrap().list));
            assert_eq!(shell.lifecycle.live_count(), 2);
        });
    });
}

#[gpui::test]
fn ipc_executor_is_cold_until_first_answer_and_then_serves_the_committed_pane(
    cx: &mut TestAppContext,
) {
    let (backend, _) = Backend::test_channel();
    let (send, events) = async_channel::bounded(2);
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    send.try_send(Event::Call {
        request: flea::ipc::wire::Request::Call {
            target: "flea".into(),
            function: "total".into(),
            arguments: vec![],
        },
        answer,
    })
    .unwrap();
    let window = cx.add_empty_window();
    let entity = window.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    window.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            shell.ipc_cold = true;
            shell.ipc_events = Some(events);
            shell.start_ipc_reader(window, cx);
            shell.receive(0, BackendEvent::Reply(Reply::Other), window, cx);
            assert!(shell.ipc_cold);
        })
    });
    window.run_until_parked();
    // A call before the first answer is refused at once, never run after the listing lands.
    assert_eq!(
        receive.try_recv().unwrap().reply,
        flea::ipc::registry::no_target()
    );
    window.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            shell.panes[0].as_mut().unwrap().pane.begin(
                "/files".into(),
                crate::pane::swap::Options::default(),
                0,
            );
            shell.receive(
                0,
                BackendEvent::Reply(Reply::Listed {
                    path: "/files".into(),
                    total: 1000,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                BackendEvent::Reply(Reply::Rows {
                    start: 0,
                    rows: vec![flea::protocol::RowFacts::default(); 320],
                    listing: 3,
                }),
                window,
                cx,
            );
            assert!(shell._ipc_reader.is_some());
            assert!(shell.ipc_events.is_none());
            assert!(!shell.ipc_cold);
        })
    });
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    send.try_send(Event::Call {
        request: flea::ipc::wire::Request::Call {
            target: "flea".into(),
            function: "total".into(),
            arguments: vec![],
        },
        answer,
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        receive.try_recv().unwrap().reply,
        flea::ipc::wire::Reply::value("1000")
    );
}

#[path = "visual/language_state_tests.rs"]
mod visual_language_tests;
#[path = "visual/state_tests.rs"]
mod visual_tests;

// A pane takes the catalog's words when it is built and again on every language change, so its dates never wait on a join.
#[test]
fn a_pane_host_holds_catalog_words_from_construction_and_each_language_change() {
    let catalog = |revision| Snapshot {
        revision,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let mut host = PaneHost::new("/files".into(), None, 620., &catalog(1));
    let words = Rc::clone(&host.list.borrow().view.words);
    assert!(
        words.ready,
        "a new pane holds no catalog words, so it draws no dates"
    );
    host.translated(&catalog(2));
    assert!(
        !Rc::ptr_eq(&words, &host.list.borrow().view.words),
        "a language change kept the old words"
    );
}
#[gpui::test]
fn every_flea_quit_entry_waits_for_the_window_barrier(cx: &mut TestAppContext) {
    for entry in ["close", "ipc", "key"] {
        let (backend, _) = Backend::test_channel();
        let window = cx.add_empty_window();
        window.update(|window, cx| {
            let entity = cx.new(|cx| shell_with(backend, cx));
            entity.update(cx, |shell, cx| {
                match entry {
                    "close" => {
                        shell.close(window, cx);
                    }
                    "ipc" => shell.ipc(Event::Quit, window, cx),
                    "key" => shell.key(
                        &gpui::KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("ctrl-q").unwrap(),
                            is_held: false,
                            prefer_character_input: false,
                        },
                        window,
                        cx,
                    ),
                    _ => unreachable!(),
                }
                assert_eq!(shell.lifecycle.phase(), Phase::Draining, "{entry}");
                let backend = shell.panes[0].as_ref().unwrap().backend.as_ref().unwrap();
                assert!(backend.window(0, 1).is_err(), "{entry}");
                shell.receive(0, BackendEvent::Drained, window, cx);
                assert_eq!(shell.lifecycle.phase(), Phase::Closed, "{entry}");
            });
        });
    }
}

#[gpui::test]
fn platform_quit_stops_window_tasks_and_releases_backend_ownership(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let closing = backend.closing_probe();
    let entity = cx.new(|cx| shell_with(backend, cx));
    entity.update(cx, |shell, cx| {
        shell._ipc_reader = Some(cx.spawn(async |_, _| std::future::pending::<()>().await));
        shell._locale_reader = Some(cx.spawn(async |_, _| std::future::pending::<()>().await));
        shell.deadlines[0] = Some(cx.spawn(async |_, _| std::future::pending::<()>().await));
        shell.panes[0].as_mut().unwrap().reader =
            Some(cx.spawn(async |_, _| std::future::pending::<()>().await));
        shell.stop_for_app_quit();
        // The dropped handle's quit must not arm the 30-second barrier over a closed stdin.
        assert_eq!(closing(), crate::backend::Closing::Stdin);
        assert!(shell._ipc_reader.is_none());
        assert!(shell._locale_reader.is_none());
        assert!(shell.deadlines.iter().all(Option::is_none));
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.stopped && host.reader.is_none() && host.backend.is_none());
    });
}

#[path = "input/join_tests.rs"]
mod u03;
