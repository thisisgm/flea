use super::*;
use flea::ui_logic_pv2::markdown::markdown_flow::{Face, Measure};
use gpui::{point, px, size, Bounds, TestAppContext};

fn tokens(column: bool) -> Tokens {
    Tokens {
        body: if column { 13. } else { 14. },
        theme_body: 14.,
        caption: 12.,
        gap: 9.,
        hairline: 1.,
        row_pad_x: 14.,
        row_pad_y: 7.,
        mark: 19.,
        state_mark: 40.,
        source_line: None,
        column,
    }
}

#[test]
fn vf4_rendered_column_keeps_the_board_top_padding() {
    assert_eq!(tokens(true).inset_y(), 20.);
    assert_eq!(tokens(true).source_inset_y(), 16.);
}

#[gpui::test]
fn vf4_markdown_source_uses_the_body_line_box(cx: &mut TestAppContext) {
    let theme = crate::palette::Theme::default();
    let palette = Palette {
        background: theme.background,
        code: theme.background,
        surface: theme.surface,
        foreground: theme.foreground,
        bright: theme.foreground_bright,
        muted: theme.muted,
        figure_muted: theme.muted,
        muted_frame: theme.muted_frame,
        accent_frame: theme.accent_frame,
        inks: theme.code_inks,
    };
    let (view, cx) = cx.add_window_view(move |_, cx| {
        let mut view = MarkdownView::new(
            palette,
            tokens(false),
            Faces::omarchy_default(),
            "look",
            &crate::test_catalogs::english(),
            cx,
        );
        view._tasks.clear();
        view
    });
    view.update(cx, |view, _| {
        view.source_frame(900., 600.);
        assert_eq!(view.source.frame.line, 20.);
        view.tokens.body = 18.;
        view.source_frame(900., 600.);
        assert_eq!(view.source.frame.line, 25.);
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tokens.body = 14.;
            view.active = true;
            view.view = View::Source;
            view.doc.shown = true;
            view.source
                .set_text(Arc::from("one\ntwo"), Arc::new(vec![0, 4]));
            let _ = view.prepare(
                Bounds::new(point(px(0.), px(0.)), size(px(900.), px(600.))),
                window,
                cx,
            );
            let positions: Vec<f32> = view
                .ops
                .iter()
                .filter_map(|op| match op {
                    paint::Op::Text { baseline, .. } => Some(*baseline),
                    _ => None,
                })
                .collect();
            assert_eq!(
                positions.len(),
                4,
                "both numbered source rows must be laid out"
            );
            let (a, d) = view.source.measure().extent(Face::Regular, 14.);
            let (ca, cd) = view.source.measure().extent(Face::Regular, 12.);
            let top = view.tokens.gap + view.tokens.row_pad_y;
            assert!((positions[1] - (top + (20. - a - d) / 2. + a)).abs() < 0.01);
            assert!((positions[0] - (top + (20. - ca - cd) / 2. + ca - 1.)).abs() < 0.01);
            assert!((positions[3] - positions[1] - 20.).abs() < 0.01);
        })
    });
}
