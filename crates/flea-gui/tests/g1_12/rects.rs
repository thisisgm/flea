//! The rects the path surfaces paint, read from the pure functions paint calls, at body 14 (the board) and body 20.
use super::jump::board;
use flea_gui::{
    list::Typeface,
    path::{
        field::{caret_box, frame, wash_box},
        menu::{line_top, mark_box, run_wash},
        paint::strip_pad,
        scale::Scale,
    },
};
use gpui::{point, px, size, Bounds, Pixels};

fn at20() -> Scale {
    Scale::at(&Typeface {
        body: px(20.),
        body_small: px(18.),
        caption: px(17.),
        ..Typeface::omarchy_default()
    })
}

fn rect(bounds: Bounds<Pixels>) -> (f32, f32, f32, f32) {
    (
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    )
}

#[test]
fn the_row_pitch_mark_and_label_rects_follow_the_text_size() {
    // The popup's inner origin: its window point (80, 30) plus the 1 px frame.
    let inner = point(px(81.), px(31.));
    let s = board();
    assert_eq!(
        (s.row_top(0, 0.), s.row_top(1, 0.), s.row_top(2, 10.)),
        (5., 42., 69.),
        "a 5 px pad, 37 px rows, scrolled 10"
    );
    assert_eq!(
        s.label_x(),
        41.,
        "the mark's 18 px and the 9 px gap after the 14 px pad"
    );
    assert_eq!(
        rect(mark_box(&s, inner, s.row_top(0, 0.))),
        (95., 46., 18., 18.),
        "the 18 px mark at 14, centred on 46"
    );
    let line = line_top(&s, inner, s.row_top(0, 0.));
    assert_eq!(
        f32::from(line),
        46.,
        "the 16.8 px line box centred to 10 in the row"
    );
    assert_eq!(
        rect(run_wash(&s, px(122.), line, px(30.))),
        (122., 46., 30., 17.),
        "the run wash is 17 tall on the line"
    );
    let t = at20();
    assert_eq!(
        (t.row_top(0, 0.), t.row_top(1, 0.), t.row_top(2, 10.)),
        (7., 59., 101.),
        "52 px rows at body 20"
    );
    assert_eq!(t.label_x(), 59., "the 26 px mark at 20 and the 13 px gap");
    assert_eq!(
        rect(mark_box(&t, inner, t.row_top(0, 0.))),
        (101., 51., 26., 26.),
        "the 26 px mark at 20, centred on 51"
    );
    let line = line_top(&t, inner, t.row_top(0, 0.));
    assert_eq!(
        f32::from(line),
        52.,
        "the 24 px line box centred to 14 in the row"
    );
    assert_eq!(
        rect(run_wash(&t, px(140.), line, px(30.))),
        (140., 52., 30., 24.),
        "the run wash is 24 tall at 20"
    );
}

#[test]
fn the_field_wash_and_caret_are_the_board_rects_at_fourteen_and_twenty() {
    // The path slot is the chrome less its 1 px rule: 26 px at 14, 36 px at 20.
    let slot = |height: f32| Bounds::new(point(px(80.), px(0.)), size(px(788.), px(height)));
    let s = board();
    let f = frame(slot(26.));
    assert_eq!(rect(f), (80., 2., 788., 23.), "the board's field frame");
    assert_eq!(
        rect(wash_box(f, px(100.), px(130.), &s)),
        (100., 5., 30., 16.),
        "the 16 px wash on rows 5 to 20"
    );
    // The text origin is 90 (the 1 px border and the 9 px inset), so the caret lands at 90 + 23 + 1.
    assert_eq!(
        rect(caret_box(f, px(90.), px(23.), &s)),
        (114., 5., 1., 17.),
        "the 17 px caret on rows 5 to 21, 1 px past the advance"
    );
    let t = at20();
    let f = frame(slot(36.));
    assert_eq!(
        rect(f),
        (80., 2., 788., 33.),
        "the frame at 20 is the 36 px slot less the 3 px of hairlines"
    );
    assert_eq!(
        rect(wash_box(f, px(100.), px(130.), &t)),
        (100., 7., 30., 22.),
        "the 22 px wash centred in the frame's inside at 20"
    );
    // The text origin at 20 is 80 + 1 + 13, so the caret lands at 94 + 23 + 1.
    assert_eq!(
        rect(caret_box(f, px(94.), px(23.), &t)),
        (118., 7., 1., 23.),
        "the 23 px caret centred in the same inside"
    );
}

#[test]
fn the_dual_strip_first_crumb_starts_a_row_padding_in_at_every_text_size() {
    assert_eq!(
        (strip_pad(false, &board()), strip_pad(true, &board())),
        (0., 14.),
        "the chrome has no inset, a pane its 14 px pad"
    );
    assert_eq!(
        (strip_pad(false, &at20()), strip_pad(true, &at20())),
        (0., 20.),
        "a pane's pad is PanePath's rowPaddingX, 20 at body 20"
    );
}
