//! Each Quick Look and Compare board's geometry at 1100 by 700, read off the drawn frame: g2.pv1.<board>.primary.
use super::pixels::{assert_centred, assert_own_pixels, at_board_scale};
use super::scenes::*;
use super::*;

pub fn rect(cx: &mut VisualTestContext, selector: &'static str) -> (f32, f32, f32, f32) {
    let b = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} draws"));
    (
        f32::from(b.origin.x),
        f32::from(b.origin.y),
        f32::from(b.size.width),
        f32::from(b.size.height),
    )
}

fn size_of(cx: &mut VisualTestContext, selector: &'static str) -> (f32, f32) {
    let (_, _, w, h) = rect(cx, selector);
    (w, h)
}

fn x_of(cx: &mut VisualTestContext, selector: &'static str) -> f32 {
    rect(cx, selector).0
}

fn english() -> flea::i18n::I18n {
    crate::test_catalogs::english()
}

#[gpui::test]
fn ql_text_is_the_tall_card_with_its_bar_and_lines(cx: &mut TestAppContext) {
    let (cx, _, _, _) = host(cx, ql_text(&english()), Arm::Fix);
    assert_eq!(rect(cx, "pv1-card"), (99., 24., 902., 652.));
    assert_eq!(rect(cx, "pv1-bar"), (100., 25., 900., 27.));
    // 41 px under the card's top, 31 px in, 20 px rows clear of the right edge by 20
    assert_eq!(rect(cx, "pv1-line-0"), (131., 66., 849., 20.));
    assert_eq!(rect(cx, "pv1-line-3").1, 126.);
    assert_eq!(size_of(cx, "pv1-close"), (24., 26.));
    assert_eq!(x_of(cx, "pv1-close"), 1000. - 14. - 24.);
}

#[gpui::test]
fn ql_image_is_the_picture_alone(cx: &mut TestAppContext) {
    let (cx, _, _, _) = host(cx, ql_image(), Arm::Fix);
    at_board_scale(cx);
    assert_eq!(rect(cx, "pv1-card"), (99., 63., 902., 574.));
    assert_eq!(rect(cx, "pv1-picture"), (99. + 69., 63., 765., 574.));
    assert!(cx.debug_bounds("pv1-bar").is_none());
}

#[gpui::test]
fn ql_svg_draws_the_picture_at_the_logical_fit(cx: &mut TestAppContext) {
    // #336: the vector raster changes pixels, never layout, so the fit matches ql-image exactly.
    let (cx, _, _, _) = host(cx, ql_svg(), Arm::Fix);
    assert_eq!(rect(cx, "pv1-picture"), (99. + 69., 63., 765., 574.));
    assert!(cx.debug_bounds("pv1-bar").is_none());
}

fn small_picture(vector: bool) -> Subject {
    let mut subject = if vector { ql_svg() } else { ql_image() };
    if let Body::Image { w, h, .. } = &mut subject.body {
        *w = 60.;
        *h = 40.;
    }
    subject.shape.w = 60.;
    subject.shape.h = 40.;
    subject
}

#[gpui::test]
fn ql_small_svg_fills_the_card_and_small_png_keeps_its_own_pixels(cx: &mut TestAppContext) {
    // EY-64: a 60x40 vector in the 902x574 card fills along the limiting axis, aspect kept.
    let (cx, _, _, _) = host(cx, small_picture(true), Arm::Fix);
    assert_eq!(rect(cx, "pv1-card"), (99., 63., 902., 574.));
    assert_eq!(rect(cx, "pv1-picture"), (120., 63., 861., 574.));
    // A small raster draws one source pixel per device pixel, centred in the card; only zoom enlarges it.
    let (cx, _, _, _) = host(cx, small_picture(false), Arm::Fix);
    assert_own_pixels(cx, "pv1-picture", (60., 40.));
    assert_centred(cx, "pv1-picture", "pv1-card");
}

fn sized(w: f64, h: f64, vector: bool) -> Subject {
    let mut subject = small_picture(vector);
    if let Body::Image { w: sw, h: sh, .. } = &mut subject.body {
        (*sw, *sh) = (w, h);
    }
    (subject.shape.w, subject.shape.h) = (w, h);
    subject
}

#[gpui::test]
fn ql_picture_size_matrix_never_enlarges_a_raster(cx: &mut TestAppContext) {
    // Geometry matrix (EY-330): source class x window size; a raster fits down or stays 1:1, a vector fills.
    let sources = [(60., 40.), (800., 600.), (600., 2000.), (4000., 3000.)];
    for (window_w, window_h) in [(1100., 700.), (2560., 1400.)] {
        for (sw, sh) in sources {
            for vector in [false, true] {
                let (cx, _, _, _) = host(cx, sized(sw, sh, vector), Arm::Fix);
                cx.simulate_resize(size(px(window_w), px(window_h)));
                cx.update(|window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                });
                cx.run_until_parked();
                let (_, _, cw, ch) = rect(cx, "pv1-card");
                let (_, _, w, h) = rect(cx, "pv1-picture");
                let fit = (cw as f64 / sw).min(ch as f64 / sh);
                let device = cx.update(|window, _| window.scale_factor()) as f64;
                let scale = if vector { fit } else { fit.min(1. / device) };
                let cell = format!("{sw}x{sh} vector={vector} in {window_w}x{window_h}");
                assert!((w as f64 - sw * scale).abs() <= 1., "{cell}: width {w}");
                assert!((h as f64 - sh * scale).abs() <= 1., "{cell}: height {h}");
                assert!(
                    vector || (w as f64) * device <= sw && (h as f64) * device <= sh,
                    "{cell}: enlarged"
                );
            }
        }
    }
}

#[gpui::test]
fn ql_native_pixels_survive_an_odd_centered_room(cx: &mut TestAppContext) {
    for (sw, sh) in [(60., 40.), (3., 3.)] {
        let (cx, _, _, _) = host(cx, sized(sw, sh, false), Arm::Fix);
        cx.update(|window, _| window.set_scale_factor(1.));
        cx.simulate_resize(size(px(1101.), px(701.)));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        let (_, _, width, _) = rect(cx, "pv1-card");
        assert_eq!(
            (width.round() as u32) % 2,
            1,
            "fixture card must have odd device width"
        );
        assert_own_pixels(cx, "pv1-picture", (sw as f32, sh as f32));
        assert_centred(cx, "pv1-picture", "pv1-card");
    }
}

// A symlink whose name and picture are a 60x40 vector or raster, drawn under the link bar.
fn small_link(vector: bool) -> Subject {
    let mut subject = ql_symlink(&english());
    let small = small_picture(vector);
    if let (Body::Link { path, w, h, .. }, Body::Image { path: picture, .. }) =
        (&mut subject.body, &small.body)
    {
        *path = picture.clone();
        *w = 60.;
        *h = 40.;
    }
    subject
}

#[gpui::test]
fn ql_small_svg_link_fills_under_the_bar_and_small_png_link_keeps_its_size(
    cx: &mut TestAppContext,
) {
    // The 547 px room under the 27 px bar limits a 60x40 vector: it draws 547 tall, aspect kept.
    let (cx, _, _, _) = host(cx, small_link(true), Arm::Fix);
    let (_, y, w, h) = rect(cx, "pv1-picture");
    assert_eq!(
        (y, h),
        (63. + 27., 547.),
        "the vector fills the room under the bar"
    );
    assert!((w - 547. * 1.5).abs() <= 1., "the aspect is kept: {w}");
    let (cx, _, _, _) = host(cx, small_link(false), Arm::Fix);
    assert_own_pixels(cx, "pv1-picture", (60., 40.));
}

#[gpui::test]
fn ql_symlink_puts_the_target_under_the_filled_bar(cx: &mut TestAppContext) {
    let (cx, _, _, _) = host(cx, ql_symlink(&english()), Arm::Fix);
    at_board_scale(cx);
    assert_eq!(rect(cx, "pv1-bar"), (99., 63., 902., 27.));
    assert_eq!(rect(cx, "pv1-picture"), (99. + 86., 63. + 27., 729., 547.));
    assert_eq!(x_of(cx, "pv1-close"), 1001. - 14. - 24.);
}

#[gpui::test]
fn ql_raw_draws_the_embedded_picture_over_its_caption(cx: &mut TestAppContext) {
    let i18n = english();
    let subject = ql_raw(&i18n);
    assert!(
        matches!(&subject.body, Body::Raw { caption, .. } if caption == "Embedded preview \u{b7} 1620 \u{d7} 1080")
    );
    let (cx, _, _, _) = host(cx, subject, Arm::Fix);
    at_board_scale(cx);
    assert_eq!(rect(cx, "pv1-picture"), (99. + 41., 63., 821., 547.));
    assert_eq!(rect(cx, "pv1-caption"), (99., 637. - 27., 902., 27.));
}

#[gpui::test]
fn ql_error_keeps_the_pdf_strip_and_draws_the_error_recipe(cx: &mut TestAppContext) {
    let i18n = english();
    let subject = ql_error(&i18n);
    assert!(
        matches!(&subject.body, Body::CannotShow { sentence, .. } if sentence == "This file could not be read.")
    );
    let (cx, _, _, _) = host(cx, subject, Arm::Fix);
    assert_eq!(rect(cx, "pv1-bar"), (99., 63., 902., 27.));
    assert_eq!(rect(cx, "pv1-recipe"), (99., 90., 902., 547.));
    // close at the right, then fit, then the two disabled zooms, 9 px apart
    assert_eq!(x_of(cx, "pv1-close"), 963.);
    assert_eq!(x_of(cx, "pv1-control-2"), 963. - 33.);
    assert_eq!(x_of(cx, "pv1-disabled-pv1-mark:plus"), 963. - 66.);
    assert_eq!(x_of(cx, "pv1-disabled-pv1-mark:minus"), 963. - 99.);
    assert_eq!(size_of(cx, "pv1-open-with").1, 30.);
}

#[gpui::test]
fn visual_geometry_ql_unsupported_is_the_calm_recipe_with_no_bar(cx: &mut TestAppContext) {
    let i18n = english();
    let subject = ql_unsupported(&i18n);
    assert!(
        matches!(&subject.body, Body::CannotShow { sentence, .. } if sentence == "This file cannot be previewed.")
    );
    let (cx, _, _, _) = host(cx, subject, Arm::Fix);
    assert_eq!(rect(cx, "pv1-card"), (99., 63., 902., 574.));
    assert!(cx.debug_bounds("pv1-bar").is_none());
    assert_eq!(rect(cx, "pv1-recipe"), (100., 64., 900., 572.));
    let (x, y, w, h) = rect(cx, "pv1-open-with");
    assert_eq!((x + w / 2., h), (550., 30.));
    assert_eq!(y.round(), 377., "Open With board top");
}

#[gpui::test]
fn compare_draws_the_title_copy_patch_and_the_rows(cx: &mut TestAppContext) {
    let i18n = english();
    let subject = compare_diff(&i18n);
    assert!(matches!(&subject.body, Body::Diff { title, copy, .. }
        if title == "notes-a.md and notes-b.md \u{b7} 12 lines differ" && copy == "Copy Patch"));
    assert_eq!(subject.mark(), flea_gui::g2::pv1::glyph::Glyph::Markdown);
    let (cx, _, _, _) = host(cx, subject, Arm::Fix);
    // the bar lies over the card's frame, edge to edge
    assert_eq!(rect(cx, "pv1-bar"), (99., 63., 902., 27.));
    assert_eq!(rect(cx, "pv1-diff-0"), (100., 64. + 38., 900., 22.));
    assert_eq!(rect(cx, "pv1-diff-1").1, 64. + 60.);
    assert!(cx.debug_bounds("pv1-copy-patch").is_some());
    assert_eq!(x_of(cx, "pv1-close"), 1001. - 14. - 24.);
}

fn refusal(cx: &mut TestAppContext, subject: Subject, title: &str, sentence: &str) {
    assert!(
        matches!(&subject.body, Body::Refused { title: t, sentence: s } if t == title && s == sentence)
    );
    let (cx, _, _, _) = host(cx, subject, Arm::Fix);
    assert_eq!(rect(cx, "pv1-bar"), (100., 64., 900., 26.));
    assert_eq!(size_of(cx, "pv1-close"), (24., 25.));
    assert_eq!(x_of(cx, "pv1-close"), 1000. - 13. - 24.);
    assert_eq!(rect(cx, "pv1-recipe"), (100., 91., 900., 545.));
    assert!(cx.debug_bounds("pv1-open-with").is_none());
}

#[gpui::test]
fn compare_binary_says_so(cx: &mut TestAppContext) {
    let i18n = english();
    refusal(
        cx,
        compare_binary(&i18n),
        "shot-a.png and shot-b.png",
        "These files are binary, so there is nothing to compare.",
    );
}

#[gpui::test]
fn compare_identical_says_so(cx: &mut TestAppContext) {
    let i18n = english();
    refusal(
        cx,
        compare_identical(&i18n),
        "notes-a.md and notes-b.md",
        "The two files are identical.",
    );
}

#[gpui::test]
fn compare_large_names_the_limit(cx: &mut TestAppContext) {
    let i18n = english();
    refusal(
        cx,
        compare_large(&i18n),
        "dump-a.log and dump-b.log",
        "These files are over 1.0 MB, too large to compare.",
    );
}

#[test]
fn each_strip_mark_reads_its_accessible_name() {
    use flea_gui::g2::pv1::glyph::Glyph;
    let i18n = english();
    let named = [
        (Glyph::Close, "Close"),
        (Glyph::Minus, "Zoom out"),
        (Glyph::Plus, "Zoom in"),
        (Glyph::Maximize, "Expand"),
    ];
    for (glyph, name) in named {
        assert_eq!(
            glyph.access_key().map(|k| i18n.tr(k)).as_deref(),
            Some(name)
        );
    }
}
