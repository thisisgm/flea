use super::*;

#[test]
fn visual_geometry_column_code_keeps_17px_pitch_and_board_origin() {
    let face = crate::list::Typeface::omarchy_default();
    let look = Look {
        text_limit: 1000,
        remote: false,
        theme: crate::palette::Theme::default(),
        metrics: crate::list::Metrics::for_typeface(&face),
        face,
        muted: crate::palette::Theme::default().muted,
        mark: ("file-text".into(), &[]),
    };
    let code = look.code(true);
    assert_eq!(code.metrics.row, 17.);
    assert_eq!(
        code.metrics.top, 9.,
        "frame top 41 plus padding 9 and glyph bearing 3 gives ink y53"
    );
    assert_eq!(look.code(false).text_left(gpui::px(7.2)), 9.);
}
