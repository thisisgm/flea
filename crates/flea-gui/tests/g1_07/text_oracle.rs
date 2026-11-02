//! Independently exhaust middle elisions and keep the widest real glyph run inside the cell budget.
use super::frames::typeface;
use flea_gui::row::{cell_count, middle_elide};
use gpui::{GlyphRun, Hsla, TextRun, Window};

pub fn fitted_name(window: &Window, name: &str, cells: usize, advance: f32) -> String {
    let face = typeface();
    (0..=cell_count(name))
        .rev()
        .map(|budget| middle_elide(name, budget))
        .find(|text| {
            let style = TextRun {
                len: text.len(),
                font: face.font.clone(),
                color: Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window
                .text_system()
                .layout_line(text, face.body, &[style], None);
            f32::from(line.width) <= cells as f32 * advance + 0.01
        })
        .expect("an empty elision fits")
        .into_owned()
}

/// The run's glyphs are exactly the shaping of this text in the track's face.
pub fn says(window: &Window, run: &GlyphRun, text: &str) -> bool {
    let face = typeface();
    let style = TextRun {
        len: text.len(),
        font: face.font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let want = window
        .text_system()
        .layout_line(text, face.body, &[style], None);
    let glyphs = |runs: &[gpui::ShapedRun]| {
        runs.iter()
            .flat_map(|run| {
                run.glyphs
                    .iter()
                    .map(move |glyph| (run.font_id, glyph.id, glyph.position))
            })
            .collect::<Vec<_>>()
    };
    run.line().len == want.len
        && run.line().font_size == want.font_size
        && run.line().width == want.width
        && glyphs(&run.line().runs) == glyphs(&want.runs)
}

#[test]
fn says_refuses_matching_glyph_ids_at_a_wrong_face_size_or_position() {
    use super::frames::{held, open, real_cx};
    use gpui::{px, LineLayout};
    use std::rc::Rc;
    use std::sync::Arc;

    let mut cx = real_cx();
    let handle = open(&mut cx, Rc::from([]), held(vec![]), 400., 200.);
    handle
        .update(&mut cx, |_, window, _| {
            let face = typeface();
            let text = "same glyphs";
            let style = TextRun {
                len: text.len(),
                font: face.font.clone(),
                color: Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window.text_system().layout_line(
                text,
                face.body,
                std::slice::from_ref(&style),
                None,
            );
            let normal = GlyphRun::new(Arc::clone(&line));
            assert!(says(window, &normal, text));
            let large = GlyphRun::new(window.text_system().layout_line(
                text,
                face.body * 2.,
                std::slice::from_ref(&style),
                None,
            ));
            let ids = |run: &GlyphRun| {
                run.line()
                    .runs
                    .iter()
                    .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.id))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                ids(&normal),
                ids(&large),
                "the old glyph-only comparison accepts this control"
            );
            assert!(
                !says(window, &large, text),
                "a larger run must not pass the budget oracle"
            );
            let same_geometry = || LineLayout {
                font_size: line.font_size,
                width: line.width,
                ascent: line.ascent,
                descent: line.descent,
                runs: line.runs.clone(),
                len: line.len,
            };
            let mut shifted = same_geometry();
            shifted.runs[0].glyphs[0].position.x += px(1.);
            assert!(
                !says(window, &GlyphRun::new(Arc::new(shifted)), text),
                "glyph positions are part of the oracle"
            );
            let mut wrong_face = same_geometry();
            // Font fallbacks may alias weights; corrupt only the identity to test the comparison deterministically.
            wrong_face.runs[0].font_id.0 ^= 1;
            assert!(
                !says(window, &GlyphRun::new(Arc::new(wrong_face)), text),
                "font identity is part of the oracle even when metrics and glyph IDs match"
            );
        })
        .expect("the oracle window is open");
}
