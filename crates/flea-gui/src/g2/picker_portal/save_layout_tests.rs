//! EY-263: picker-save's supplied name has a full field, visible selected stem and no caret.
use super::{tests::fixture::fixture, text::EntryHost};
use crate::{list::Palette, palette::Theme};
use gpui::{px, size, Hsla, TestAppContext};

fn same(a: Hsla, b: Hsla) -> bool {
    (a.h - b.h).abs() < 1e-3
        && (a.s - b.s).abs() < 1e-3
        && (a.l - b.l).abs() < 1e-3
        && (a.a - b.a).abs() < 1e-3
}

const TEXT_FILES: &str = r#", "filters":[{"label":"Text files","globs":["*.txt"]}]"#;

#[gpui::test]
fn picker_gm_narrow_save_moves_the_filter_to_its_own_row(cx: &mut TestAppContext) {
    for width in [720., 1100.] {
        let f = fixture(
            cx,
            &format!(r#"{{"mode":"save","folder":"/d","name":"sa-d3-new.txt"{TEXT_FILES}}}"#),
            None,
        );
        f.cx.simulate_resize(size(px(width), px(700.)));
        f.cx.run_until_parked();
        let field =
            f.cx.debug_bounds("pk1-filename")
                .expect("the mounted Save As field");
        let filter =
            f.cx.debug_bounds("picker-filter")
                .expect("the caller's filter trigger");
        assert_eq!(
            f32::from(field.size.width),
            300.,
            "the field keeps picker-save's 300 px, so sa-d3-new.txt shows whole at {width} px"
        );
        if width < 1100. {
            assert!(
                filter.bottom() <= field.top(),
                "at {width} px the trigger takes its own row above the answers, as v0.3.12 does"
            );
        } else {
            assert!(
                field.right() <= filter.left() && filter.top() < field.bottom(),
                "at {width} px the trigger shares the answer row after the field"
            );
        }
    }
}

#[gpui::test]
fn ey263_supplied_save_name_paints_in_a_full_width_field(cx: &mut TestAppContext) {
    for (width, filters) in [(1100., ""), (720., ""), (1100., TEXT_FILES)] {
        let mut f = fixture(
            cx,
            &format!(r#"{{"mode":"save","folder":"/d","name":"sa-d3-new.txt"{filters}}}"#),
            None,
        );
        f.cx.simulate_resize(size(px(width), px(700.)));
        f.cx.run_until_parked();
        let frame =
            f.cx.debug_bounds("pk1-filename")
                .expect("the mounted Save As field");
        let field_width = f32::from(frame.size.width);
        if width >= 1100. {
            assert_eq!(field_width, 300., "picker-save's preferred field width");
        } else {
            assert!(
                field_width > 20. && field_width <= 300.,
                "the narrower chooser shrinks the field without collapsing its text area"
            );
        }
        assert_eq!(f32::from(frame.size.height), 30.);
        if !filters.is_empty() {
            let filter =
                f.cx.debug_bounds("picker-filter")
                    .expect("the caller's filter trigger");
            assert!(
                frame.right() <= filter.left(),
                "the field ends before the filter trigger at {width} px"
            );
        }
        let field = f.read(|picker| picker.browse.filename.clone().unwrap());
        let selected_width = f.cx.update(|window, cx| {
            let field = field.read(cx);
            assert_eq!(field.entry.text, "sa-d3-new.txt");
            assert_eq!(
                field.entry.selected,
                0..9,
                "the stem is selected, keeping .txt outside the wash"
            );
            assert!(field.focus.is_focused(window));
            let drawn = field.drawn().borrow();
            let bounds = drawn.bounds.expect("the input was laid out");
            assert_eq!(
                bounds.left() - frame.left(),
                px(10.),
                "picker-save's 9 px padding inside its 1 px frame"
            );
            let run = drawn
                .line
                .run
                .as_ref()
                .expect("the supplied name was shaped");
            assert!(
                bounds.size.width >= run.line().width,
                "the complete supplied name fits in the field's content bounds"
            );
            assert_eq!(
                drawn.line.scroll,
                px(0.),
                "the supplied short name is visible from its first glyph"
            );
            let (from, to) = drawn.line.entry_span(&field.entry, &field.entry.selected);
            assert!(to > from, "the selected stem has a visible advance");
            f32::from(to - from)
        });
        let (quads, palette, scale) = f.cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            (
                window.painted_quads(),
                Palette::inherited(
                    &window.text_style(),
                    cx.try_global::<Theme>().copied().unwrap_or_default(),
                ),
                window.scale_factor(),
            )
        });
        assert!(
            quads.iter().any(|quad| {
                quad.background.as_solid().is_some_and(|color| same(color, palette.accent.opacity(0.24)))
                    && (quad.bounds.size.height.0 - 20. * scale).abs() < 1e-3
                    && (quad.bounds.size.width.0 - selected_width * scale).abs() <= 1.
            }),
            "the selected stem paints on picker-save's 24% accent wash: scale={scale}, wanted={:?}, quads={:?}",
            palette.accent.opacity(0.24),
            quads.iter().filter(|q| q.bounds.size.height.0 <= 30. * scale).map(|q| (q.bounds.size, q.background.as_solid())).collect::<Vec<_>>()
        );
        assert!(
            !quads.iter().any(|quad| {
                quad.bounds.origin.x.0 >= f32::from(frame.left()) * scale
                    && quad.bounds.origin.x.0 <= f32::from(frame.right()) * scale
                    && quad.bounds.origin.y.0 >= f32::from(frame.top()) * scale
                    && quad.bounds.origin.y.0 <= f32::from(frame.bottom()) * scale
                    && (quad.bounds.size.width.0 - scale).abs() < 1e-3
                    && (quad.bounds.size.height.0 - 16. * scale).abs() < 1e-3
            }),
            "a selected stem has no caret"
        );
    }
}
