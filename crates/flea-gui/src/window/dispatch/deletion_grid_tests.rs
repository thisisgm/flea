//! A delete in Grid lands the cursor on the deleted tile's slot at the same place on screen, in every order of the delete, thumbnail and watch replies.
use super::super::{
    rename_tests::{close, draw, ready_root, receive, sent},
    test_shell::{land_on, set_cursor},
};
use crate::{
    backend::{Backend, Event},
    grid::element::pictures::Policy,
    grid::Shown,
    window::{navigate::Step, startup::View},
};
use fixture::{images, pending, thumbnail, windows};
use flea::protocol::Reply;
use gpui::{point, px, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, TestAppContext};
use std::time::Duration;

#[path = "deletion_grid_fixture.rs"]
mod fixture;

// The folder's size, the tile each case deletes, and the tile that takes its slot.
const IMAGES: usize = 1000;
const TARGET: usize = 500;
const NEXT: usize = TARGET + 1;
// Where a keyboard case leaves the cursor: two tiles after the marked tile.
const KEYBOARD_CURSOR: usize = TARGET + 2;
// Where a pointer case rests, beside the tile, so only the click puts the cursor on it.
const POINTER_REST: usize = TARGET - 1;
// Clock time that lets the visible tiles' thumbnail requests go out.
const SETTLE: Duration = Duration::from_millis(200);
// One frame of clock time, and how many frames the replacement tile must hold still.
const FRAME: Duration = Duration::from_millis(100);
const FRAMES: usize = 10;
// How long, in milliseconds, a delayed case holds the listing reply back.
const LISTING_DELAY: u64 = 100;

fn deletion(
    cx: &mut TestAppContext,
    thumbnail_first: bool,
    delay: u64,
    pointer: bool,
    fresh: bool,
) {
    let (shell, cx, mut lines) = ready_root(cx, "default");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            if fresh {
                shell.set_view(View::Columns, window, cx);
            }
            assert_eq!(shell.step(0, Step::Refresh(None), window, cx), Ok(true));
        })
    });
    let (first, _) = pending(&shell, cx);
    land_on(
        &shell,
        cx,
        0,
        "/files",
        images()[..first].to_vec(),
        IMAGES,
        2,
    );
    if !fresh {
        set_cursor(&shell, cx, TARGET);
    }
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.selected = if fresh || pointer {
                Vec::new()
            } else {
                vec![TARGET]
            };
            if pointer {
                host.pane.cursor = IMAGES - 1;
            }
            if !fresh {
                host.pane.reveal_cursor();
                // A pointer case rests beside the tile, so only the click puts the cursor on it; a keyboard case leaves the cursor off the marked tile, so only the delete anchor brings it to the slot.
                host.pane.cursor = if pointer {
                    POINTER_REST
                } else {
                    KEYBOARD_CURSOR
                };
            }
            shell.set_view(View::Grid, window, cx)
        })
    });
    windows(&shell, cx, &images());
    if fresh {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
                let layout = grid.view.layout(Shown::All(IMAGES));
                let delta = layout.row_top(TARGET / layout.columns) - layout.height / 2.;
                let event = ScrollWheelEvent {
                    position: grid.state.borrow().bounds.center(),
                    delta: ScrollDelta::Pixels(point(px(0.), px(-delta))),
                    ..Default::default()
                };
                shell.wheel(&event, window, cx);
            })
        });
        windows(&shell, cx, &images());
    }
    receive(
        &shell,
        cx,
        Backend::test_reply_line(r#"{"t":"fsinfo","path":"/files","class":""}"#),
    );
    draw(cx);
    cx.executor().advance_clock(SETTLE);
    windows(&shell, cx, &images());
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let policy = Policy::read(&shell.launch.settings.data);
        let files = host
            .list
            .borrow()
            .pictures
            .policy_files(&host.pane.held, &host.path, policy);
        assert_eq!(
            files.get(&NEXT),
            Some(&None),
            "a visible image has a thumbnail request in flight"
        );
    });
    let y = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        host.grid
            .as_ref()
            .unwrap()
            .state
            .borrow()
            .drawn_row(TARGET)
            .unwrap()
            .cell
            .y
    });
    if pointer || fresh {
        let (at, cursor) = shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let state = host.grid.as_ref().unwrap().state.borrow();
            let tile = state.drawn_row(TARGET).unwrap();
            let at = state.bounds.origin
                + point(
                    px(tile.slot.x + tile.slot.width / 2.),
                    px(tile.slot.y + tile.slot.height / 2.),
                );
            (at, host.pane.cursor)
        });
        assert_ne!(
            cursor, TARGET,
            "the click, not the setup, puts the cursor on the tile"
        );
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    }
    sent(&mut lines);
    cx.simulate_keystrokes("delete");
    let requests = sent(&mut lines);
    assert!(
        requests
            .iter()
            .any(|line| line.contains(r#""c":"trash""#) && line.contains(&format!("[{TARGET}]"))),
        "Delete trashes tile {TARGET}'s file: {requests:?}"
    );
    receive(
        &shell,
        cx,
        Backend::test_reply_line(r#"{"t":"trashed","ok":1,"failed":0}"#),
    );
    if fresh {
        receive(
            &shell,
            cx,
            Event::Changed {
                path: "/files".into(),
            },
        );
    }
    if thumbnail_first {
        thumbnail(&shell, cx);
        draw(cx);
    }
    receive(
        &shell,
        cx,
        crate::backend::Event::Reply(Reply::Listed {
            path: "/files".into(),
            total: IMAGES - 1,
        }),
    );
    cx.executor().advance_clock(Duration::from_millis(delay));
    draw(cx);
    let mut rows = images();
    rows.remove(TARGET);
    let (first, listing) = pending(&shell, cx);
    receive(
        &shell,
        cx,
        crate::backend::Event::Reply(Reply::Rows {
            start: 0,
            rows: rows[..first].to_vec(),
            listing,
        }),
    );
    if fresh && !thumbnail_first {
        windows(&shell, cx, &rows);
        cx.executor().advance_clock(SETTLE);
        windows(&shell, cx, &rows);
    }
    if !thumbnail_first {
        thumbnail(&shell, cx);
    }
    for frame in 0..FRAMES {
        cx.executor().advance_clock(FRAME);
        if fresh
            && shell.read_with(cx, |shell, _| {
                shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .pane
                    .swap
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.total.is_none())
            })
        {
            let (first, listing) = pending(&shell, cx);
            receive(
                &shell,
                cx,
                Event::Reply(Reply::Listed {
                    path: "/files".into(),
                    total: IMAGES - 1,
                }),
            );
            receive(
                &shell,
                cx,
                Event::Reply(Reply::Rows {
                    start: 0,
                    rows: rows[..first].to_vec(),
                    listing,
                }),
            );
        }
        windows(&shell, cx, &rows);
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.cursor, TARGET, "frame {frame}, delay {delay}");
            assert_eq!(
                host.pane.held.row(TARGET).unwrap().name,
                format!("image{NEXT:04}.png")
            );
            // Not on the fresh route: its watch re-read asks where its mark is, which this driver does not answer.
            if !fresh {
                assert_eq!(host.selected, [TARGET], "frame {frame}: the lone mark");
            }
            let grid = host.grid.as_ref().unwrap();
            let state = grid.state.borrow();
            let after = state
                .drawn_row(TARGET)
                .expect("replacement tile is drawn")
                .cell
                .y;
            assert!(
                (after - y).abs() < 0.01,
                "frame {frame}, delay {delay}: tile moved {y} to {after}, scroll {}",
                state.scroll
            );
        });
    }
    if fresh {
        shell.read_with(cx, |shell, _| {
            assert!(shell.panes[0].as_ref().unwrap().nav.watch.rereads > 0);
        });
    }
    close(&shell, cx);
}

#[gpui::test]
fn gh476_delete_after_thumbnail_lands_grid_cursor_on_the_deleted_slot(cx: &mut TestAppContext) {
    deletion(cx, true, 0, false, false);
}

#[gpui::test]
fn gh476_thumbnail_after_delete_lands_grid_cursor_on_the_deleted_slot(cx: &mut TestAppContext) {
    deletion(cx, false, 0, false, false);
}

#[gpui::test]
fn gh476_delayed_delete_after_thumbnail_lands_grid_cursor_on_the_deleted_slot(
    cx: &mut TestAppContext,
) {
    deletion(cx, true, LISTING_DELAY, false, false);
}

#[gpui::test]
fn gh476_thumbnail_after_delayed_delete_lands_grid_cursor_on_the_deleted_slot(
    cx: &mut TestAppContext,
) {
    deletion(cx, false, LISTING_DELAY, false, false);
}

#[gpui::test]
fn gh476_pointer_delete_with_thumbnail_before_rows(cx: &mut TestAppContext) {
    deletion(cx, true, LISTING_DELAY, true, false);
}

#[gpui::test]
fn gh476_pointer_delete_with_thumbnail_after_rows(cx: &mut TestAppContext) {
    deletion(cx, false, LISTING_DELAY, true, false);
}

#[gpui::test]
fn gh476_reply_order_columns_wheel_click_delete_thumbnail_then_watch(cx: &mut TestAppContext) {
    deletion(cx, true, LISTING_DELAY, false, true);
}

#[gpui::test]
fn gh476_reply_order_columns_wheel_click_delete_watch_then_thumbnail(cx: &mut TestAppContext) {
    deletion(cx, false, LISTING_DELAY, false, true);
}
