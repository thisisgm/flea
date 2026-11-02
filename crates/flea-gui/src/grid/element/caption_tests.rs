//! Fixed-pitch Grid captions retain the top-aligned font baseline and its measured extent.
use super::*;

#[gpui::test]
fn visual_geometry_grid_caption_ink_matches_both_docs_boards(cx: &mut gpui::TestAppContext) {
    use gpui::AppContext;
    let handle = cx.add_window(|_, _| gpui::Empty);
    cx.update_window(handle.into(), |_, window, _| {
        let state = GridState::new(gpui::font("JetBrains Mono"));
        let run = label_run("archive-tar-bz2.", &state, px(13.), window);
        let line = 12. * 15. / 11.;
        for top in [0., line] {
            assert_eq!(
                caption_baseline(&run, px(top), window),
                window.pixel_snap(px(top + 2.) + run.line().ascent)
            );
        }
        let row = flea::protocol::RowFacts {
            name: "archive-tar-bz2.tar.bz2".into(),
            ..Default::default()
        };
        let shaping = crate::grid::tile::Shaping {
            listing: 1,
            font: state.font.clone(),
            size: px(13.),
            scale: window.scale_factor(),
        };
        let tile = crate::grid::tile::prepare(&row, 16, 2, &shaping, window);
        assert!((tile.text_height(1, line) - (line + 1.)).abs() < 0.001);
        assert!((tile.text_height(2, line) - (2. * line + 1.)).abs() < 0.001);
        assert_eq!(tile.text_height(0, line), 0.);
    })
    .unwrap();
}
