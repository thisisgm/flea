//! Search replacement preserves file identities across backend numbering changes.
use super::tests::{edit, open, reply, Handle};
use crate::backend::Event;
use flea::protocol::{Reply, RowFacts};
use gpui::TestAppContext;

fn listing(cx: &mut TestAppContext, handle: Handle, stamp: usize, names: &[&str]) {
    let generation = edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .swap
            .session
            .generation
    });
    let count = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.first_count()
    });
    reply(
        cx,
        handle,
        0,
        &format!(r#"{{"t":"listed","path":"/files","n":1000,"gen":{generation}}}"#),
    );
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            Event::Session {
                generation,
                reply: Reply::Rows {
                    start: 0,
                    listing: stamp,
                    rows: (0..count)
                        .map(|at| RowFacts {
                            name: names
                                .get(at)
                                .map_or_else(|| format!("filler-{at}.txt"), |name| (*name).into()),
                            ..Default::default()
                        })
                        .collect(),
                },
            },
            window,
            cx,
        );
    });
}

fn setup(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (handle, read) = open(cx);
    edit(cx, handle, |shell, window, cx| {
        shell.search_update(
            0,
            None,
            |search, host| search.open_query("txt", host),
            window,
            cx,
        );
    });
    listing(cx, handle, 11, &["a/same.txt", "b/same.txt"]);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![0, 1, 900];
        host.input.selection.changed(&host.selected);
        host.pane.cursor = 1;
        host.pane.scroll_to(17.);
    });
    (handle, read)
}

fn remapped(cx: &mut TestAppContext, handle: Handle, from: usize, to: usize, gen: usize) {
    remapped_rows(cx, handle, (from, to, gen), "[7,800]");
}

fn remapped_rows(
    cx: &mut TestAppContext,
    handle: Handle,
    stamps: (usize, usize, usize),
    rows: &str,
) {
    let (from, to, gen) = stamps;
    reply(
        cx,
        handle,
        0,
        &format!(
            r#"{{"t":"searchremap","ok":true,"gen":{gen},"from":{from},"listing":{to},"rows":{rows},"cursor":7}}"#
        ),
    );
}

fn pending(cx: &mut TestAppContext, handle: Handle) -> (bool, bool, Vec<usize>) {
    edit(cx, handle, |shell, _, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        (
            host.search.kept.is_some(),
            shell.wheel_blocked(cx),
            host.selected.clone(),
        )
    })
}

#[gpui::test]
fn searchkeep_replacement_remaps_off_window_marks_and_cursor_offset(cx: &mut TestAppContext) {
    let (handle, mut read) = setup(cx);
    let (gen, offset) = edit(cx, handle, |shell, _, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        (
            pane.swap.session.generation,
            pane.cursor as f32 * pane.row_height() - pane.scroll_y,
        )
    });
    listing(cx, handle, 12, &["b/same.txt", "new.txt"]);
    let requests: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert!(
        requests
            .iter()
            .any(|line| line.contains(r#""c":"searchremap""#)
                && line.contains(r#""rows":[0,1,900]"#)),
        "{requests:?}"
    );
    remapped(cx, handle, 11, 12, gen);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.selected, [7, 800]);
        assert_eq!(host.pane.cursor, 7);
        assert_eq!(
            host.pane.cursor as f32 * host.pane.row_height() - host.pane.scroll_y,
            offset
        );
        assert_eq!(host.pane.state.query, "txt");
    });
}

#[gpui::test]
fn searchkeep_repeated_replacements_do_not_overwrite_the_source(cx: &mut TestAppContext) {
    let (handle, mut read) = setup(cx);
    let gen = edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .swap
            .session
            .generation
    });
    listing(cx, handle, 12, &["other.txt"]);
    listing(cx, handle, 13, &["b/same.txt"]);
    let requests: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert_eq!(
        requests
            .iter()
            .filter(|line| line.contains(r#""c":"searchremap""#))
            .count(),
        1
    );
    remapped_rows(cx, handle, (11, 13, gen), "[3,640]");
    assert_eq!(pending(cx, handle), (false, false, vec![3, 640]));
    remapped(cx, handle, 11, 13, gen);
    assert_eq!(pending(cx, handle), (false, false, vec![3, 640]));
}

#[gpui::test]
fn searchkeep_unappliable_answer_settles_the_pending_restoration(cx: &mut TestAppContext) {
    let (handle, mut read) = setup(cx);
    let gen = edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .swap
            .session
            .generation
    });
    listing(cx, handle, 12, &["new.txt"]);
    assert_eq!(pending(cx, handle), (true, true, vec![]));
    remapped(cx, handle, 11, 99, gen);
    assert_eq!(pending(cx, handle), (false, false, vec![]));
    std::iter::from_fn(&mut read).for_each(drop);
    listing(cx, handle, 13, &["new.txt"]);
    let requests: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert!(
        requests
            .iter()
            .any(|line| line.contains(r#""c":"searchremap""#) && line.contains(r#""from":12"#)),
        "{requests:?}"
    );
}

#[gpui::test]
fn searchkeep_stale_generation_and_other_pane_cannot_restore_marks(cx: &mut TestAppContext) {
    let (handle, _read) = setup(cx);
    let gen = edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .swap
            .session
            .generation
    });
    listing(cx, handle, 12, &["new.txt"]);
    remapped(cx, handle, 11, 12, gen + 1);
    assert_eq!(pending(cx, handle), (true, true, vec![]));
    remapped(cx, handle, 10, 12, gen);
    assert_eq!(pending(cx, handle), (true, true, vec![]));
    reply(
        cx,
        handle,
        1,
        &format!(
            r#"{{"t":"searchremap","ok":true,"gen":{gen},"from":11,"listing":12,"rows":[7],"cursor":7}}"#
        ),
    );
    assert_eq!(pending(cx, handle), (true, true, vec![]));
    edit(cx, handle, |shell, window, cx| {
        shell.search_update(
            0,
            None,
            |search, host| search.open_query("new", host),
            window,
            cx,
        );
    });
    listing(cx, handle, 14, &["new.txt"]);
    remapped(cx, handle, 11, 12, gen);
    edit(cx, handle, |shell, _, _| {
        assert!(shell.panes[0].as_ref().unwrap().selected.is_empty())
    });
}

#[gpui::test]
fn searchkeep_metadata_reply_keeps_marks_and_operation_reread_keeps_query(cx: &mut TestAppContext) {
    let (handle, _read) = setup(cx);
    edit(cx, handle, |shell, window, cx| {
        let rows = shell.panes[0].as_ref().unwrap().pane.held.rows.clone();
        shell.receive(
            0,
            Event::Reply(Reply::Rows {
                start: 0,
                listing: 11,
                rows,
            }),
            window,
            cx,
        );
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0, 1, 900]);
        shell
            .step(0, crate::window::navigate::Step::Reread, window, cx)
            .unwrap();
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.pane.swap.session.leaving);
        assert_eq!(host.search.owner.session.query, "txt");
    });
}

#[gpui::test]
fn searchkeep_scope_pane_and_dead_backend_discard_pending_restoration(cx: &mut TestAppContext) {
    for cause in ["scope", "pane", "backend"] {
        let (handle, _read) = setup(cx);
        let generation = edit(cx, handle, |shell, _, _| {
            shell.panes[0]
                .as_ref()
                .unwrap()
                .pane
                .swap
                .session
                .generation
        });
        listing(cx, handle, 12, &["new.txt"]);
        edit(cx, handle, |shell, window, cx| match cause {
            "scope" => {
                shell.search_update(0, None, |search, host| search.click_scope(host), window, cx)
            }
            "pane" => {
                shell.panes[0] = Some(crate::window::pane::PaneHost::new(
                    "/elsewhere".into(),
                    None,
                    620.,
                    &shell.locale,
                ))
            }
            _ => shell.receive(
                0,
                Event::Failed(crate::backend::Failure {
                    key: "window.backend_eof",
                    detail: String::new(),
                }),
                window,
                cx,
            ),
        });
        assert_eq!(pending(cx, handle), (false, false, vec![]), "{cause}");
        remapped(cx, handle, 11, 12, generation);
        assert_eq!(pending(cx, handle), (false, false, vec![]), "{cause}");
    }
}
