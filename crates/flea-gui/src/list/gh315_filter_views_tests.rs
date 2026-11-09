//! Issue #315 on mounted Grid and Columns, using the real backend's whole-listing answers.
#[path = "gh315_backend_fixture.rs"]
mod backend_fixture;
use crate::{
    backend::Backend,
    columns::geometry::Slot,
    grid::Shown,
    state::{tests::shell_with, Shell},
    window::{root::Root, startup::View},
};
use backend_fixture::{name, Fixture, TOTAL};
use flea::protocol::{Reply, RowFacts};
use gpui::{AppContext, Context, KeyDownEvent, Keystroke, TestAppContext, Window, WindowHandle};
use std::rc::Rc;

type Handle = WindowHandle<Root>;

fn facts(range: std::ops::Range<usize>) -> Vec<RowFacts> {
    range
        .map(|row| RowFacts {
            name: name(row),
            ..Default::default()
        })
        .collect()
}

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| f(shell, window, cx))
        })
        .unwrap()
}

fn press(cx: &mut TestAppContext, handle: Handle, key: &str, text: Option<&str>) {
    edit(cx, handle, |shell, window, cx| {
        shell.key(
            &KeyDownEvent {
                keystroke: Keystroke {
                    key: key.into(),
                    key_char: text.map(Into::into),
                    ..Default::default()
                },
                is_held: false,
                prefer_character_input: false,
            },
            window,
            cx,
        )
    });
}

fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn serve(cx: &mut TestAppContext, handle: Handle) {
    let owed = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.requests.outstanding
    });
    if let Some(w) = owed {
        edit(cx, handle, |shell, window, cx| {
            shell.receive(
                0,
                crate::backend::Event::Reply(Reply::Rows {
                    start: w.start,
                    listing: 1,
                    rows: facts(w.start..(w.start + w.count).min(TOTAL)),
                }),
                window,
                cx,
            )
        });
    }
    frame(cx, handle);
}

// Read the painter, including its shaped name, rather than the filter's model alone.
fn drawn(cx: &mut TestAppContext, handle: Handle, view: View) -> Vec<(usize, usize, String)> {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        match view {
            View::Grid => host
                .grid
                .as_ref()
                .unwrap()
                .state
                .borrow()
                .drawn()
                .iter()
                .map(|row| (row.view, row.listing, row.name().unwrap_or("").to_owned()))
                .collect(),
            View::Columns => {
                let paint = host
                    .columns
                    .as_ref()
                    .unwrap()
                    .painters
                    .get(Slot::Active)
                    .unwrap()
                    .borrow();
                let drawn = paint.drawn.as_ref().unwrap();
                paint
                    .visible()
                    .map(|row| {
                        (
                            drawn.position_of(row).unwrap(),
                            row,
                            paint.prepared(row).unwrap().label.clone(),
                        )
                    })
                    .collect()
            }
            _ => unreachable!(),
        }
    })
}

fn check_names(rows: &[(usize, usize, String)]) {
    assert!(!rows.is_empty(), "a proper listing is drawn");
    for (_, row, painted) in rows {
        assert_eq!(
            *painted,
            name(*row),
            "row {row} painted a stale or empty name"
        );
    }
}

fn mount(cx: &mut TestAppContext, view: View) -> (Handle, impl FnMut() -> Option<String>) {
    let (backend, mut read) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            shell.panes[0].as_mut().unwrap().pane.held = Rc::new(crate::pane::Held {
                listing: 1,
                total: TOTAL,
                start: 0,
                rows: facts(0..320),
            });
            shell.set_view(view, window, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    frame(cx, handle);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(
            crate::pane::swap::CAP_MS + 1,
        ));
    frame(cx, handle);
    assert_eq!(drawn(cx, handle, view).first().map(|row| row.1), Some(0));
    while read().is_some() {}
    (handle, read)
}

// Page down from the top until the cursor sits a third of the way into the folder, past the first held window.
fn go_deep(cx: &mut TestAppContext, handle: Handle) {
    press(cx, handle, "home", None);
    for _ in 0..200 {
        if edit(cx, handle, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().pane.cursor >= 1666
        }) {
            break;
        }
        press(cx, handle, "pagedown", None);
        frame(cx, handle);
        serve(cx, handle);
    }
    edit(cx, handle, |shell, _, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        assert!(
            (1666..2500).contains(&pane.cursor),
            "the second query starts deep"
        );
        assert!(pane.held.start > 320, "the held window has left the top");
    });
}

// The real backend answers the last request for this query; the answer lands, a frame draws and any owed window is served.
fn answer(
    cx: &mut TestAppContext,
    handle: Handle,
    fixture: &Fixture,
    read: &mut impl FnMut() -> Option<String>,
    query: &str,
) -> String {
    let mut asked = Vec::new();
    while let Some(line) = read() {
        asked.push(line);
    }
    // Sample: {"c":"filter","query":"file-2345","id":9}.
    let id = asked
        .iter()
        .rev()
        .find(|line| {
            flea::json::field_str(line, "c").as_deref() == Some("filter")
                && flea::json::field_str(line, "query").as_deref() == Some(query)
        })
        .and_then(|line| flea::json::field_usize(line, "id"))
        .unwrap_or_else(|| panic!("the query never reached the backend: {asked:?}"));
    let answer = fixture.answer(query, id);
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(&answer), window, cx)
    });
    frame(cx, handle);
    serve(cx, handle);
    answer
}

fn regression(cx: &mut TestAppContext, view: View) {
    let fixture = Fixture::new();
    let (handle, mut read) = mount(cx, view);

    // First query starts at the top; second starts one third down, outside the original held rows.
    for (phase, query, target) in [(1, "file-2345", 2344), (2, "0300", 299)] {
        if phase == 2 {
            go_deep(cx, handle);
        }
        while read().is_some() {}
        press(cx, handle, "/", Some("/"));
        for character in query.chars() {
            let key = character.to_string();
            press(cx, handle, &key, Some(&key));
        }
        let answer = answer(cx, handle, &fixture, &mut read, query);
        let narrowed = drawn(cx, handle, view);
        assert_eq!(
            narrowed
                .iter()
                .map(|row| (row.0, row.1))
                .collect::<Vec<_>>(),
            vec![(0, target)],
            "the off-window match draws at the top: {answer}"
        );
        check_names(&narrowed);
        edit(cx, handle, |shell, _, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, target);
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            assert_eq!(footer.counts.to_string(), "1 of 5,000 shown");
        });

        while read().is_some() {}
        press(cx, handle, "escape", None);
        edit(cx, handle, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(
                host.filter.shown.is_none(),
                "one Escape restores the whole listing"
            );
            assert!(
                shell.slots.strips[0].is_none(),
                "the strip closed with that same Escape"
            );
            assert_eq!(host.pane.cursor, target);
        });
        frame(cx, handle);
        serve(cx, handle);
        let restored = drawn(cx, handle, view);
        check_names(&restored);
        assert!(restored.len() > 1, "the unfiltered viewport refilled");
        assert!(
            restored.iter().any(|row| row.1 == target),
            "the cursor is visible after one Escape"
        );
        for pair in restored.windows(2) {
            assert_eq!(
                pair[1].1,
                pair[0].1 + 1,
                "the restored listing is contiguous"
            );
        }
        assert!(
            restored.iter().all(|row| row.0 == row.1),
            "unfiltered positions keep listing identity"
        );
        while let Some(line) = read() {
            assert_ne!(
                flea::json::field_str(&line, "c").as_deref(),
                Some("filter"),
                "clearing must not need a second filter answer"
            );
        }
    }
}

#[gpui::test]
fn gh315_grid_off_window_match_and_deep_one_escape_refill(cx: &mut TestAppContext) {
    regression(cx, View::Grid);
}

#[gpui::test]
fn gh315_columns_off_window_match_and_deep_one_escape_refill(cx: &mut TestAppContext) {
    regression(cx, View::Columns);
}

// After one Escape: the Grid cursor tile's top in the viewport, its height, the viewport height, and whether the next row shows.
fn grid_escape_place(cx: &mut TestAppContext, stepwise: bool) -> (f32, f32, f32, bool) {
    let fixture = Fixture::new();
    let (handle, mut read) = mount(cx, View::Grid);
    go_deep(cx, handle);
    while read().is_some() {}
    press(cx, handle, "/", Some("/"));
    let query = "0300";
    for end in 1..=query.len() {
        let key = &query[end - 1..end];
        press(cx, handle, key, Some(key));
        // Live typing: each keystroke's answer lands and draws before the next key, walking the cursor 0, 2, 29, 299.
        if stepwise {
            answer(cx, handle, &fixture, &mut read, &query[..end]);
        }
    }
    if !stepwise {
        answer(cx, handle, &fixture, &mut read, query);
    }
    press(cx, handle, "escape", None);
    frame(cx, handle);
    serve(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 299);
        let grid = host.grid.as_ref().unwrap();
        let layout = grid.view.layout(Shown::All(TOTAL));
        let scroll = grid.view.drawn_scroll();
        (
            layout.row_top(299 / layout.columns) - scroll,
            layout.cell_height,
            layout.height,
            layout.covers(scroll, 299 + layout.columns),
        )
    })
}

#[gpui::test]
fn gh315_grid_escape_places_the_cursor_alike_however_answers_land(cx: &mut TestAppContext) {
    let batched = grid_escape_place(cx, false);
    let stepwise = grid_escape_place(cx, true);
    assert_eq!(
        batched, stepwise,
        "one Escape places the cursor tile the same whether the answers landed per key or at once"
    );
    let (top, cell, height, next) = batched;
    // GridArea.qml placeCursor: Contain from the filtered view's top, so a cursor below it lands low and whole.
    assert!(
        top > height / 2. && top + cell <= height,
        "the cursor tile is whole near the bottom: {batched:?}"
    );
    assert!(next, "the row after the cursor shows once the strip closes");
}
