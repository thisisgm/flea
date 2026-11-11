//! Search replies retain the window requested before a live settings or geometry change.
use super::*;
use crate::g2::file_search::StripKey;
use gpui::AppContext;

fn search(
    cx: &mut TestAppContext,
    density: &str,
    contents: bool,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<String>) {
    let (handle, _) = open(cx, "list", density, 14, (1100., 700.));
    let (backend, read) = Backend::test_lines();
    handle
        .update(cx, |shell, window, cx| {
            for reply in [
                Reply::Listed {
                    path: "/files".into(),
                    total: 0,
                },
                rows(0),
            ] {
                shell.receive(0, Event::Reply(reply), window, cx);
            }
            shell.panes[0].as_mut().unwrap().backend = Some(backend);
            assert!(shell.search_action(
                if contents { "searchContents" } else { "search" },
                window,
                cx,
            ));
        })
        .unwrap();
    (handle, read)
}

fn query(cx: &mut TestAppContext, handle: WindowHandle<Shell>, text: &str) {
    handle
        .update(cx, |shell, window, cx| {
            shell.search_update(
                0,
                None,
                |search, host| search.key(StripKey::Text(text.into()), host),
                window,
                cx,
            );
        })
        .unwrap();
}

fn sent(read: &mut impl FnMut() -> Option<String>) -> (usize, usize) {
    let line = std::iter::from_fn(read)
        .filter(|line| {
            matches!(
                flea::json::field_str(line, "c").as_deref(),
                Some("searchquery" | "searchcontents")
            )
        })
        .last()
        .expect("the production search must send its query");
    (
        flea::json::field_usize(&line, "gen").unwrap(),
        flea::json::field_usize(&line, "count").unwrap(),
    )
}

fn settings(cx: &mut TestAppContext, handle: WindowHandle<Shell>, patch: &str) {
    handle
        .update(cx, |shell, window, cx| {
            let patch = flea::jsondoc::parse(patch).unwrap();
            let document = flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
            shell.receive_settings(
                super::super::settings::store::Update::Loaded {
                    document,
                    through: 0,
                    saved: false,
                },
                window,
                cx,
            );
        })
        .unwrap();
}

fn search_rows(generation: usize, count: usize) -> Reply {
    Reply::Rows {
        start: 0,
        listing: 10 + generation,
        rows: (0..count)
            .map(|at| RowFacts {
                name: format!("file-{at:04}.txt"),
                ..Default::default()
            })
            .collect(),
    }
}

fn answer(cx: &mut TestAppContext, handle: WindowHandle<Shell>, generation: usize, count: usize) {
    handle
        .update(cx, |shell, window, cx| {
            for reply in [
                Reply::Listed {
                    path: "/files".into(),
                    total: 2000,
                },
                search_rows(generation, count),
            ] {
                shell.receive(0, Event::Session { generation, reply }, window, cx);
            }
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.error.is_none(), "{:?}", host.error);
            assert_eq!(host.pane.held.total, 2000);
            assert_eq!(host.pane.held.rows.len(), count);
            assert_eq!(host.search.rows_generation, generation);
            assert!(host.pane.swap.pending.is_none());
        })
        .unwrap();
}

#[gpui::test]
fn search_window_accepts_sent_count_after_density_and_resize(cx: &mut TestAppContext) {
    let (handle, mut read) = search(cx, "compact", false);
    query(cx, handle, "file");
    let (generation, count) = sent(&mut read);
    settings(cx, handle, r#"{"density":"tight"}"#);
    cx.simulate_window_resize(handle.into(), size(px(1100.), px(1000.)));
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    handle
        .update(cx, |shell, _, _| {
            assert_ne!(shell.panes[0].as_ref().unwrap().pane.first_count(), count);
        })
        .unwrap();
    answer(cx, handle, generation, count);
}

#[gpui::test]
fn search_window_keeps_count_for_names_contents_and_walking_replies(cx: &mut TestAppContext) {
    for contents in [false, true] {
        for (before, patch) in [
            ("compact", r#"{"density":"tight"}"#),
            ("tight", r#"{"density":"compact"}"#),
            ("compact", r#"{"display":{"textSize":{"mode":16}}}"#),
        ] {
            let (handle, mut read) = search(cx, before, contents);
            query(cx, handle, "file");
            let (generation, count) = sent(&mut read);
            settings(cx, handle, patch);
            handle
                .update(cx, |shell, _, _| {
                    assert_ne!(shell.panes[0].as_ref().unwrap().pane.first_count(), count);
                })
                .unwrap();
            answer(cx, handle, generation, count);
            handle
                .update(cx, |shell, window, cx| {
                    shell.receive(
                        0,
                        Backend::test_reply_line(&format!(
                            r#"{{"t":"searched","n":2000,"scanned":2000,"ms":1,"cancelled":false,"gen":{generation},"walking":true}}"#,
                        )),
                        window,
                        cx,
                    );
                })
                .unwrap();
            // A walking search pushes another pair for the same request after the session publishes again.
            answer(cx, handle, generation, count);
        }
    }
}

#[gpui::test]
fn search_window_resize_between_listed_and_rows_keeps_the_sent_count(cx: &mut TestAppContext) {
    let (handle, mut read) = search(cx, "compact", false);
    query(cx, handle, "file");
    let (generation, count) = sent(&mut read);
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                Event::Session {
                    generation,
                    reply: Reply::Listed {
                        path: "/files".into(),
                        total: 2000,
                    },
                },
                window,
                cx,
            );
        })
        .unwrap();
    cx.simulate_window_resize(handle.into(), size(px(1100.), px(400.)));
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    handle
        .update(cx, |shell, _, _| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            assert_ne!(pane.first_count(), count);
            assert_eq!(pane.swap.pending.as_ref().unwrap().first, count);
        })
        .unwrap();
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                Event::Session {
                    generation,
                    reply: search_rows(generation, count),
                },
                window,
                cx,
            );
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.error.is_none(), "{:?}", host.error);
            assert_eq!(host.pane.held.total, 2000);
            assert_eq!(host.pane.held.rows.len(), count);
            assert_eq!(host.search.rows_generation, generation);
            assert!(host.pane.swap.pending.is_none());
        })
        .unwrap();
}

#[gpui::test]
fn search_window_new_generation_owns_its_count_and_still_rejects_bad_rows(cx: &mut TestAppContext) {
    for delta in [-1, 1] {
        let (handle, mut read) = search(cx, "compact", false);
        query(cx, handle, "file");
        let (old_generation, old_count) = sent(&mut read);
        settings(cx, handle, r#"{"density":"tight"}"#);
        query(cx, handle, "x");
        let (generation, count) = sent(&mut read);
        assert!(generation > old_generation);
        assert_ne!(count, old_count);
        settings(cx, handle, r#"{"density":"compact"}"#);
        handle
            .update(cx, |shell, window, cx| {
                for generation in [old_generation, generation] {
                    shell.receive(
                        0,
                        Event::Session {
                            generation,
                            reply: Reply::Listed {
                                path: "/files".into(),
                                total: 2000,
                            },
                        },
                        window,
                        cx,
                    );
                    if generation == old_generation {
                        assert!(shell.panes[0].as_ref().unwrap().pane.swap.pending.is_none());
                    }
                }
                let pane = &mut shell.panes[0].as_mut().unwrap().pane;
                assert_eq!(pane.swap.pending.as_ref().unwrap().first, count);
                assert_eq!(
                    pane.receive_session(
                        rows(count.checked_add_signed(delta).unwrap()),
                        generation,
                        0,
                    ),
                    Err("pane.reply_window".into()),
                );
                assert_eq!(pane.held.total, 0);
            })
            .unwrap();
        answer(cx, handle, generation, count);
    }
}
