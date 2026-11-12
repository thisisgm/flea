//! A refused name brings its whole row and sentence inside the owning viewport.
use super::{close, draw, ready, sent};
use crate::{
    backend::{Backend, Event},
    window::{dispatch::test_shell, startup::View},
};
use gpui::TestAppContext;
use std::time::Duration;

#[path = "rename_column_bottom_tests.rs"]
mod columns;

fn refusal(cx: &mut TestAppContext, view: View, side: usize, cut: bool) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let rows: Vec<_> = (0..120)
        .map(|at| test_shell::row(&format!("f{at:03}.txt"), false))
        .collect();
    test_shell::land(&shell, cx, "/files", rows.clone(), 2);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(view, window, cx)));
    if side == 1 {
        test_shell::land_on(&shell, cx, 1, "/files", rows, 120, 2);
        let (backend, output) = Backend::test_lines();
        shell.update(cx, |shell, cx| {
            shell.panes[1].as_mut().unwrap().backend = Some(backend);
            shell.focus_pane(1, cx);
        });
        lines = Box::new(output);
    }
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(400));
        draw(cx);
    }
    cx.simulate_keystrokes("end");
    draw(cx);
    let index = shell.read_with(cx, |shell, _| {
        shell.panes[side].as_ref().unwrap().pane.cursor
    });
    assert_eq!(index, 119);
    cx.simulate_keystrokes("f2");
    draw(cx);
    if cut {
        shell.update(cx, |shell, _| {
            let frame = shell.rename.edit.as_ref().unwrap().line.frame.unwrap();
            let host = shell.panes[side].as_mut().unwrap();
            // Leave the last row partly below the viewport while keeping its editor alive.
            if view == View::Grid {
                host.grid.as_mut().unwrap().view.scroll -= f32::from(frame.size.height);
            } else {
                host.pane
                    .scroll_to(host.pane.scroll_y - f32::from(frame.size.height));
            }
        });
        draw(cx);
    }
    let before = shell.read_with(cx, |shell, _| {
        let host = shell.panes[side].as_ref().unwrap();
        if view == View::Grid {
            host.grid.as_ref().unwrap().view.scroll
        } else {
            host.pane.scroll_y
        }
    });
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("f000.txt");
    sent(&mut lines);
    cx.simulate_keystrokes("enter");
    assert_eq!(sent(&mut lines).len(), 1, "the backend owns the refusal");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                side,
                Event::Refused {
                    operation: "rename".into(),
                    path: "/files/f119.txt".into(),
                    message: "file exists".into(),
                    mode: 0,
                },
                window,
                cx,
            );
        })
    });
    draw(cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    draw(cx);
    let (after, band) = shell.read_with(cx, |shell, _| {
        let edit = shell.rename.edit.as_ref().unwrap();
        let sentence = edit.line.error_area.expect("the refusal sentence is drawn");
        let host = shell.panes[side].as_ref().unwrap();
        let clip = if view == View::Columns {
            host.columns
                .as_ref()
                .unwrap()
                .painters
                .get(crate::columns::geometry::Slot::Active)
                .unwrap()
                .borrow()
                .drawn
                .as_ref()
                .unwrap()
                .bounds
        } else if view == View::Grid {
            host.grid.as_ref().unwrap().state.borrow().bounds
        } else {
            host.list.borrow().drawn.bounds
        };
        assert!(edit.line.frame.unwrap().top() >= clip.top());
        assert!(
            sentence.bottom() <= clip.bottom(),
            "{view:?}/{side}/cut={cut}: sentence {sentence:?} escapes {clip:?}"
        );
        if view == View::Grid {
            return (host.grid.as_ref().unwrap().view.scroll, shell.bands[side]);
        }
        let grow = shell.grows[side];
        let row = if view == View::Columns {
            host.columns_row_height()
        } else {
            host.list.borrow().drawn.metrics.row
        };
        let height = f32::from(clip.size.height);
        let expected = (grow.top(index, row) + grow.height(index, row) - height).max(before);
        assert!(
            (host.pane.scroll_y - expected).abs() < 0.01,
            "the smallest scroll contains the whole grown row"
        );
        (host.pane.scroll_y, grow.band)
    });
    assert!(after >= before && band > 0.);
    if view != View::Grid || cut {
        assert!(after > before);
    }
    cx.simulate_keystrokes("enter");
    draw(cx);
    let cleared = shell.read_with(cx, |shell, _| {
        let host = shell.panes[side].as_ref().unwrap();
        if view == View::Grid {
            host.grid.as_ref().unwrap().view.scroll
        } else {
            host.pane.scroll_y
        }
    });
    if view == View::Grid {
        assert_eq!(cleared, after, "clearing does not restore the old scroll");
    }
    assert!(
        cleared <= after,
        "clearing only clamps to the ordinary content extent"
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    shell.update(cx, |shell, _| {
        assert_eq!(shell.bands[side], 0., "closing the editor clears the band");
        let host = shell.panes[side].as_mut().unwrap();
        if view == View::Grid {
            let closed = host.grid.as_ref().unwrap().view.scroll;
            assert_eq!(closed, after, "closing does not restore the old scroll");
            return;
        }
        let closed = host.pane.scroll_y;
        host.pane.scroll_by(0.);
        assert!(
            (closed - host.pane.scroll_y).abs() < 0.01,
            "{view:?}/{side}/cut={cut}: the closed band left the scroll at {closed}, past the ordinary end {}",
            host.pane.scroll_y
        );
    });
    close(&shell, cx);
}

#[gpui::test]
fn rename_bottom_list_refusal(cx: &mut TestAppContext) {
    refusal(cx, View::List, 0, false);
}

#[gpui::test]
fn rename_bottom_columns_refusal(cx: &mut TestAppContext) {
    refusal(cx, View::Columns, 0, false);
}

#[gpui::test]
fn rename_bottom_dual_left_refusal(cx: &mut TestAppContext) {
    refusal(cx, View::Dual, 0, false);
}

#[gpui::test]
fn rename_bottom_dual_right_refusal(cx: &mut TestAppContext) {
    refusal(cx, View::Dual, 1, false);
}

#[gpui::test]
fn rename_bottom_list_cut_refusal(cx: &mut TestAppContext) {
    refusal(cx, View::List, 0, true);
}

#[gpui::test]
fn rename_bottom_columns_cut_refusal(cx: &mut TestAppContext) {
    refusal(cx, View::Columns, 0, true);
}

#[gpui::test]
fn rename_bottom_dual_cut_refusal(cx: &mut TestAppContext) {
    refusal(cx, View::Dual, 1, true);
}

#[gpui::test]
fn rename_bottom_dual_left_cut_refusal(cx: &mut TestAppContext) {
    refusal(cx, View::Dual, 0, true);
}

#[gpui::test]
fn rename_bottom_grid_refusal(cx: &mut TestAppContext) {
    refusal(cx, View::Grid, 0, false);
}

#[gpui::test]
fn rename_bottom_grid_cut_refusal(cx: &mut TestAppContext) {
    refusal(cx, View::Grid, 0, true);
}
