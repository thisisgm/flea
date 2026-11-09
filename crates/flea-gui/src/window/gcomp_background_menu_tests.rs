//! Background presses use the mounted pane ground even when no row element is painted.
use super::*;
use crate::columns::geometry::Slot;
use gpui::{point, px, Modifiers, MouseButton, Pixels, Point, VisualTestContext};

fn listing(cx: &mut TestAppContext, view: View, empty: bool) -> Handle {
    let handle = if view == View::Columns {
        let (handle, _requests) = batch::columns(cx);
        edit(cx, handle, |shell, window, cx| {
            crate::window::columns_tests::host::settle(shell, window, cx);
            shell.panes[0].as_mut().unwrap().backend = None;
        });
        handle
    } else {
        let handle = open(cx);
        edit(cx, handle, |shell, window, cx| {
            shell.set_view(view, window, cx)
        });
        handle
    };
    if empty {
        edit(cx, handle, |shell, window, cx| {
            let path = shell.panes[0].as_ref().unwrap().path.clone();
            for reply in [
                flea::protocol::Reply::Listed { path, total: 0 },
                flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 99,
                    rows: vec![],
                },
            ] {
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
            }
        });
    }
    frame(cx, handle);
    handle
}

fn ground(cx: &mut TestAppContext, handle: Handle, low: bool) -> Point<Pixels> {
    edit(cx, handle, |shell, window, _| {
        let host = shell.panes[0].as_ref().unwrap();
        if shell.shown_view() == View::Columns {
            let paint = host
                .columns
                .as_ref()
                .unwrap()
                .painters
                .get(Slot::Active)
                .unwrap()
                .borrow();
            let bounds = paint.drawn.as_ref().unwrap().bounds;
            return point(
                bounds.center().x,
                if low {
                    bounds.bottom() - px(10.)
                } else {
                    bounds.center().y
                },
            );
        }
        let mut bounds = shell.geometry(window).panes[0];
        if shell.shown_view() == View::List {
            bounds = shell
                .geometry(window)
                .rows(0, false, shell.slots.strip_height_for(0));
        }
        point(
            px(bounds.x + 60.),
            px(bounds.y + if low { bounds.height - 10. } else { 20. }),
        )
    })
}

fn right_press(cx: &mut TestAppContext, handle: Handle, at: Point<Pixels>) {
    VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
        at,
        MouseButton::Right,
        Modifiers::none(),
    );
}

fn background(cx: &mut TestAppContext, handle: Handle, at: Point<Pixels>) -> Vec<String> {
    right_press(cx, handle, at);
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let capture = shell
            .menu
            .captured
            .as_ref()
            .expect("blank ground opens a menu");
        assert!(capture.background);
        assert!(capture.rows.is_empty());
        assert_eq!(capture.path, shell.panes[0].as_ref().unwrap().path);
        let menu = shell.menu.menu.borrow();
        assert_eq!(menu.face, crate::g2::context_menus::menu::Face::Background);
        let card = menu.card.as_ref().expect("background menu is painted");
        assert_eq!(card.frame.x, f32::from(at.x), "menu anchors at pointer x");
        assert!(
            (card.frame.y - f32::from(at.y)).abs() < 1.
                || (card.frame.y + card.frame.h - f32::from(at.y)).abs() < 1.,
            "menu opens below pointer or flips above it"
        );
        card.entries
            .iter()
            .map(|entry| entry.id.to_string())
            .collect()
    })
}

fn empty_ground(view: View) {
    let mut cx = crate::list::bench::real_cx();
    let handle = listing(&mut cx, view, true);
    edit(&mut cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().pane.state.kind,
            crate::pane::Kind::Empty
        );
    });
    for low in [false, true] {
        let at = ground(&mut cx, handle, low);
        assert!(!background(&mut cx, handle, at).is_empty());
        VisualTestContext::from_window(handle.into(), &cx).simulate_keystrokes("escape");
    }
}

#[test]
fn bgmenu_empty_list_ground() {
    empty_ground(View::List);
}

#[test]
fn bgmenu_empty_grid_ground() {
    empty_ground(View::Grid);
}

#[test]
fn bgmenu_empty_columns_ground() {
    empty_ground(View::Columns);
}

#[test]
fn bgmenu_below_rows_preserves_marks_and_row_targets_in_every_view() {
    for view in [View::List, View::Grid, View::Columns] {
        let mut cx = crate::list::bench::real_cx();
        let handle = listing(&mut cx, view, false);
        edit(&mut cx, handle, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().selected = vec![1]
        });
        frame(&mut cx, handle);
        let at = ground(&mut cx, handle, true);
        background(&mut cx, handle, at);
        VisualTestContext::from_window(handle.into(), &cx).simulate_keystrokes("escape");
        let at = edit(&mut cx, handle, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.selected, [1]);
            match view {
                View::Grid => {
                    let state = host.grid.as_ref().unwrap().state.borrow();
                    let tile = &state.drawn()[0];
                    state.bounds.origin
                        + point(
                            px(tile.slot.x + tile.slot.width / 2.),
                            px(tile.slot.y + tile.slot.height / 2.),
                        )
                }
                View::Columns => {
                    let paint = host
                        .columns
                        .as_ref()
                        .unwrap()
                        .painters
                        .get(Slot::Active)
                        .unwrap()
                        .borrow();
                    let bounds = paint.drawn.as_ref().unwrap().bounds;
                    point(
                        bounds.left() + px(60.),
                        bounds.top() + px(crate::list::pane::ROW_HEIGHT / 2.),
                    )
                }
                _ => {
                    let frame = host.input.drawn.unwrap().frame;
                    point(
                        frame.bounds.left() + px(60.),
                        frame.bounds.top() + px(frame.row / 2.),
                    )
                }
            }
        });
        let (backend, _requests) = Backend::test_lines();
        edit(&mut cx, handle, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().backend = Some(backend);
        });
        right_press(&mut cx, handle, at);
        edit(&mut cx, handle, |shell, _, _| {
            let capture = shell
                .menu
                .captured
                .as_ref()
                .expect("row press opens row menu");
            assert!(!capture.background);
            assert_eq!(capture.rows, [0]);
        });
    }
}

#[test]
fn bgmenu_empty_grid_ground_starts_below_the_filter_strip() {
    let mut cx = crate::list::bench::real_cx();
    let handle = listing(&mut cx, View::Grid, true);
    VisualTestContext::from_window(handle.into(), &cx).simulate_keystrokes("/");
    frame(&mut cx, handle);
    let (strip, pane) = edit(&mut cx, handle, |shell, window, _| {
        (
            shell.slots.strip_height_for(0),
            shell.geometry(window).panes[0],
        )
    });
    assert!(
        strip > 10.,
        "the filter strip is mounted over the empty grid"
    );
    // The lowest pixels of the pane sit past the strip-shortened ground unless it starts below the strip.
    let at = point(px(pane.x + 60.), px(pane.y + pane.height - 5.));
    assert!(!background(&mut cx, handle, at).is_empty());
}
