//! The cursor row's edge in a real window: the quad the paint site draws, never the getter that names its ink.
use super::board::Board;
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
                    quad.bounds.size.width.as_f32() == Board::default().edge * scale
                        && quad.bounds.size.height.as_f32() == Board::default().row_h * scale
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
        (0., Board::default().top() * scale),
        "the edge quad sits on the cursor row's left beneath the header"
    );
    assert_eq!(
        colour,
        Some(ink.accent_frame),
        "the cursor edge paints in the accent frame"
    );
    assert_ne!(colour, Some(ink.accent), "the edge is not the accent");
    assert_ne!(
        colour,
        Some(ink.foreground),
        "the edge is not the foreground"
    );
}

#[test]
fn ey380_the_trash_rows_strip_and_header_follow_the_text_size() {
    let mut cx = real_cx();
    cx.update(|cx| cx.set_global(crate::list::Typeface::omarchy_default().at_base(20)));
    let (window, _) = opened(&mut cx, &[row("a.txt", None)]);
    let edges = cx
        .update_window(window.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let scale = window.scale_factor();
            window
                .painted_quads()
                .into_iter()
                .filter(|quad| quad.background.as_solid() == Some(ink().accent_frame))
                .map(|quad| {
                    (
                        quad.bounds.origin.y.as_f32() / scale,
                        quad.bounds.size.width.as_f32() / scale,
                        quad.bounds.size.height.as_f32() / scale,
                    )
                })
                .collect::<Vec<_>>()
        })
        .expect("the pane window is open");
    // At 20 the list's row is 42 and the chrome 37, as TrashView.qml takes Theme.fileRowHeight and chromeHeight.
    assert_eq!(
        edges,
        vec![(74., 2., 42.)],
        "the cursor edge spans a 42 row under a 37 strip and a 37 header"
    );
}

#[test]
fn the_trash_board_at_text_size_14_keeps_the_boards_numbers() {
    let b = Board::default();
    assert_eq!(
        [
            b.strip_h,
            b.pad_x,
            b.gap,
            b.cbtn_w,
            b.cbtn_glyph,
            b.button_h,
            b.button_pad
        ],
        [27., 14., 9., 24., 16., 20., 9.],
        "the strip, its back slot and Empty Trash…"
    );
    assert_eq!(
        [b.header_h, b.slot, b.location_w, b.deleted_w],
        [27., 19., 210., 135.],
        "the column header"
    );
    assert_eq!(
        [b.row_h, b.glyph, b.edge, b.menu_drop],
        [31., 19., 2., 3.],
        "the rows and the menu's drop"
    );
}

#[test]
fn adv_trash_text_size_keeps_the_full_pane_height_when_the_headers_grow() {
    let mut cx = real_cx();
    let (window, _) = opened(&mut cx, &[row("a.txt", None)]);
    window
        .update(&mut cx, |pane, _, cx| {
            pane.set_face(&crate::list::Typeface::omarchy_default().at_base(20), cx);
            assert_eq!(pane.view().height, 572., "646 minus two 37 px headers");
            pane.set_face(&crate::list::Typeface::omarchy_default(), cx);
            pane.resize(908., 60., cx);
            pane.set_face(&crate::list::Typeface::omarchy_default().at_base(20), cx);
            assert_eq!(pane.view().height, 0.);
            pane.set_face(&crate::list::Typeface::omarchy_default(), cx);
            assert_eq!(
                pane.view().height,
                6.,
                "a clamped viewport retains its original pane height"
            );
        })
        .unwrap();
}

#[test]
fn adv_trash_text_size_draws_only_a_complete_viewport() {
    use super::tests::{answer, open, sent};
    let mut cx = real_cx();
    let (window, calls) = open(&mut cx);
    window
        .update(&mut cx, |pane, window, cx| {
            window.set_rem_size(gpui::px(20.));
            pane.set_face(&crate::list::Typeface::omarchy_default().at_base(20), cx);
            pane.resize(908., 646., cx);
            pane.open("", cx);
        })
        .unwrap();
    let fields = |start: usize, count: usize| {
        let rows = (start..start + count)
            .map(|i| row(&format!("item-{i}.txt"), None))
            .collect::<Vec<_>>();
        format!(
            r#", "total":1000,"start":{start},"present":[],"rows":[{}]"#,
            rows.join(",")
        )
    };
    // The backend answers a window with exactly the rows it was asked for.
    let requested = |calls: &_| {
        let request = sent(calls)
            .into_iter()
            .rev()
            .find(|s| s.contains(r#""op":"window""#))
            .unwrap();
        let request = flea::jsondoc::parse(&request).unwrap();
        let number = |key| request.get(key).unwrap().as_f64().unwrap() as usize;
        (number("start"), number("count"))
    };
    let count = window
        .update(&mut cx, |pane, _, _| pane.view().window_rows())
        .unwrap();
    answer(&mut cx, window, &calls, "list", &fields(0, count));
    answer(
        &mut cx,
        window,
        &calls,
        "summary",
        r#", "bytes":0,"partial":false"#,
    );
    window
        .update(&mut cx, |pane, _, cx| pane.wheel(4200., cx))
        .unwrap();
    let (start, count) = requested(&calls);
    answer(&mut cx, window, &calls, "window", &fields(start, count));
    window
        .update(&mut cx, |pane, window, cx| {
            window.set_rem_size(gpui::px(14.));
            pane.set_face(&crate::list::Typeface::omarchy_default(), cx);
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    // The held margin covers the smaller pitch's viewport, so the new pitch draws at once and whole.
    window
        .update(&mut cx, |pane, _, _| {
            assert!(pane.view().covers_viewport());
            let paint = pane.paint.borrow();
            assert_eq!((paint.board.row_h, paint.scroll_y), (31., 3100.));
            let end = ((paint.scroll_y + pane.view().height) / paint.board.row_h).ceil() as usize;
            assert!((100..end).all(|i| paint.line(i).is_some()));
        })
        .unwrap();
}
