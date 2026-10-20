//! The cursor row's edge in a real window: the quad the paint site draws, never the getter that names its ink.
use super::board::{EDGE, HEADER_H, ROW_H, STRIP_H};
use super::tests::{ink, opened, row};
use crate::list::bench::real_cx;
use gpui::AppContext;

#[test]
fn the_cursor_edge_paints_in_the_accent_frame() {
    let mut cx = real_cx();
    let (window, _) = opened(&mut cx, &[row("a.txt", None)]);
    let ink = ink();
    let (bounds, colour, scale) = cx
        .update_window(window.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let scale = window.scale_factor();
            let mut edges: Vec<_> = window
                .painted_quads()
                .into_iter()
                .filter(|quad| {
                    quad.bounds.size.width.as_f32() == EDGE * scale
                        && quad.bounds.size.height.as_f32() == ROW_H * scale
                })
                .map(|quad| (quad.bounds, quad.background.as_solid()))
                .collect();
            assert_eq!(edges.len(), 1, "the cursor row paints one edge quad");
            let (bounds, colour) = edges.pop().unwrap();
            (bounds, colour, scale)
        })
        .expect("the pane window is open");
    assert_eq!(
        (bounds.origin.x.as_f32(), bounds.origin.y.as_f32()),
        (0., (STRIP_H + HEADER_H) * scale),
        "the edge quad sits on the cursor row's left beneath the header"
    );
    assert_eq!(colour, Some(ink.accent_frame), "the cursor edge paints in the accent frame");
    assert_ne!(colour, Some(ink.accent), "the edge is not the accent");
    assert_ne!(colour, Some(ink.foreground), "the edge is not the foreground");
}
