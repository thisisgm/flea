//! EY-20: a names search keeps Grid's tiles under the strip, as v0.3.12 GridArea does; contents draws List blocks.
use super::tests::{control_key, edit, frame, key, open, reply, Handle};
use crate::{backend::Event as BackendEvent, window::startup::View};
use flea::protocol::RowFacts;
use gpui::TestAppContext;

#[path = "search_long_query_tests.rs"]
mod long_query;

fn answer(cx: &mut TestAppContext, handle: Handle, generation: usize, names: &[&str]) {
    answer_at(cx, handle, generation, "/files", names);
}

pub(super) fn answer_at(
    cx: &mut TestAppContext,
    handle: Handle,
    generation: usize,
    path: &str,
    names: &[&str],
) {
    let n = names.len();
    reply(
        cx,
        handle,
        0,
        &format!(
            r#"{{"t":"listed","path":"{}","n":{n},"gen":{generation}}}"#,
            flea::json::escape(path)
        ),
    );
    edit(cx, handle, |shell, window, cx| {
        let rows = names
            .iter()
            .map(|name| RowFacts {
                name: (*name).into(),
                icon: "text-plain".into(),
                ..Default::default()
            })
            .collect();
        let reply = flea::protocol::Reply::Rows {
            start: 0,
            listing: 10 + generation,
            rows,
        };
        shell.receive(0, BackendEvent::Session { generation, reply }, window, cx)
    });
    reply(
        cx,
        handle,
        0,
        &format!(
            r#"{{"t":"searched","n":{n},"scanned":11,"ms":1,"cancelled":false,"gen":{generation},"walking":false}}"#
        ),
    );
}

/// The Grid's laid-out height and the top of its painted input surface.
fn grid_box(cx: &mut TestAppContext, handle: Handle) -> (f32, f32) {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.drawn_view, Some(View::Grid), "the pane draws tiles");
        let grid = host.grid.as_ref().expect("a mounted Grid");
        let frame = host.input.frame.expect("the Grid's painted input surface");
        (grid.view.size.1, f32::from(frame.bounds.top()))
    })
}

fn view(cx: &mut TestAppContext, handle: Handle) -> (View, bool) {
    edit(cx, handle, |shell, _, _| {
        let active = shell.panes[0].as_ref().unwrap().search.owner.active();
        (shell.view, active)
    })
}

#[gpui::test]
fn ey20_names_search_keeps_grid_tiles_under_the_strip(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    edit(cx, handle, |shell, window, cx| {
        shell.set_view(View::Grid, window, cx)
    });
    frame(cx, handle);
    let (rest_height, rest_top) = grid_box(cx, handle);
    for (name, text) in [("f", "f"), ("a", "a")] {
        key(cx, handle, name, Some(text));
    }
    assert_eq!(view(cx, handle), (View::Grid, true), "f keeps Grid");
    let generation = edit(cx, handle, |shell, _, _| {
        let session = &shell.panes[0].as_ref().unwrap().search.owner.session;
        assert_eq!(
            session.query, "a",
            "the strip over Grid takes the typed key"
        );
        session.gen as usize
    });
    answer(cx, handle, generation, &["A", "alpha.txt"]);
    frame(cx, handle);
    let strip = edit(cx, handle, |shell, window, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        let names: Vec<_> = host
            .pane
            .held
            .rows
            .iter()
            .map(|row| row.name.as_str())
            .collect();
        assert_eq!(names, ["A", "alpha.txt"]);
        assert!(
            shell.search_header(0, window, cx).is_some(),
            "the strip is mounted"
        );
        let host = shell.panes[0].as_ref().unwrap();
        crate::list::Metrics::for_typeface(&host.list.borrow().typeface).header
    });
    assert!(strip > 0.);
    let (height, top) = grid_box(cx, handle);
    assert_eq!(
        (height, top),
        (rest_height - strip, rest_top + strip),
        "tiles sit under the strip"
    );
}

#[gpui::test]
fn ey20_contents_from_grid_draws_list_blocks_and_returns_to_grid(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    edit(cx, handle, |shell, window, cx| {
        shell.set_view(View::Grid, window, cx)
    });
    frame(cx, handle);
    key(cx, handle, "f", Some("f"));
    assert_eq!(view(cx, handle), (View::Grid, true));
    control_key(cx, handle, "g");
    assert_eq!(
        view(cx, handle),
        (View::List, true),
        "a contents block is a List row"
    );
    control_key(cx, handle, "g");
    assert_eq!(
        view(cx, handle),
        (View::Grid, true),
        "names again draws tiles"
    );
    key(cx, handle, "escape", None);
    assert_eq!(view(cx, handle), (View::Grid, false));
    control_key(cx, handle, "g");
    assert_eq!(
        view(cx, handle),
        (View::List, true),
        "Ctrl+G in Grid opens contents in List"
    );
    key(cx, handle, "escape", None);
    assert_eq!(
        view(cx, handle),
        (View::Grid, false),
        "leaving returns to Grid"
    );
    frame(cx, handle);
    assert!(grid_box(cx, handle).0 > 0.);
}

/// Per keystroke: the key, a 300-row answer and one drawn frame, timed on the real text system for each origin view.
#[test]
#[ignore = "timing probe, run with --ignored --nocapture"]
fn ey20_keystroke_cost_by_origin_view() {
    for origin in [View::List, View::Grid] {
        let mut cx = crate::list::bench::real_cx();
        let (handle, _read) = open(&mut cx);
        edit(&mut cx, handle, |shell, window, cx| {
            shell.set_view(origin, window, cx)
        });
        frame(&mut cx, handle);
        key(&mut cx, handle, "f", Some("f"));
        let names: Vec<String> = (0..300)
            .map(|at| format!("Docs/alpha-{at:03}.txt"))
            .collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut costs = Vec::new();
        let letters = ["a", "l", "p", "h", "a"];
        for round in 0..60 {
            if round > 0 && round % letters.len() == 0 {
                for _ in letters {
                    key(&mut cx, handle, "backspace", None);
                }
            }
            let letter = letters[round % letters.len()];
            let started = std::time::Instant::now();
            key(&mut cx, handle, letter, Some(letter));
            let generation = edit(&mut cx, handle, |shell, _, _| {
                shell.panes[0].as_ref().unwrap().search.owner.session.gen as usize
            });
            answer(&mut cx, handle, generation, &names);
            frame(&mut cx, handle);
            costs.push(started.elapsed().as_secs_f64() * 1000.);
            let query = edit(&mut cx, handle, |shell, _, _| {
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .search
                    .owner
                    .session
                    .query
                    .clone()
            });
            assert!(!query.is_empty() && "alpha".starts_with(&query));
            assert!(
                names
                    .iter()
                    .all(|name| crate::g2::file_search::run::run_in(name, &query).is_some()),
                "timed answers must match the actual query {query:?}"
            );
        }
        let drawn = edit(&mut cx, handle, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().drawn_view
        });
        costs.drain(..10);
        costs.sort_by(f64::total_cmp);
        let at = |q: f64| costs[((costs.len() - 1) as f64 * q) as usize];
        eprintln!(
            "KEYSTROKE origin={origin:?} drawn={drawn:?} n={} median_ms={:.3} p90_ms={:.3} max_ms={:.3}",
            costs.len(),
            at(0.5),
            at(0.9),
            at(1.)
        );
    }
}
