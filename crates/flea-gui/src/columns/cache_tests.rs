//! Columns name cuts: a late filter run drawn whole at both budgets (EY-437), and a cut where the glyphs reach the column (I50-26).
use super::{Row, Shaping, TrackCache};
use flea::protocol::RowFacts;
use gpui::{font, px, TestAppContext};

const LATE: &str = "a-long-file-name-with-early-words-and-late-punctuation-[native2-late]";

#[gpui::test]
fn ey437_columns_draws_a_late_filter_run_whole_inside_the_name(cx: &mut TestAppContext) {
    let window = cx.add_empty_window();
    window.update(|window, _| {
        let shaping = Shaping {
            font: font("JetBrains Mono"),
            body: px(14.),
            caption: px(12.),
            plain_cells: 24,
            chevron_cells: 22,
            clip_cells: 0,
            size: true,
            languages: false,
            scale: 1.,
            words: Default::default(),
            locating: false,
            query: "[native2-late]".into(),
        };
        let mut cache = TrackCache::default();
        cache.begin(&shaping, 4);
        for (dir, name, plain, chevron) in [
            (
                false,
                format!("{LATE}.txt"),
                "a-long-fi\u{2026}[native2-late]",
                None,
            ),
            (
                true,
                LATE.to_owned(),
                "a-long-fi\u{2026}[native2-late]",
                Some("a-long-\u{2026}[native2-late]"),
            ),
        ] {
            let facts = RowFacts {
                name,
                dir,
                ..RowFacts::default()
            };
            let index = usize::from(dir);
            let prepared = cache.get(index, Row::Listing(&facts), false, None, &shaping, window);
            assert_eq!(prepared.label, plain);
            for (at_chevron, label) in [(false, Some(plain)), (true, chevron)] {
                let Some(label) = label else { continue };
                let matched = prepared
                    .matched(at_chevron)
                    .expect("the filter run is marked");
                assert_eq!(matched.label, label);
                assert_eq!(
                    &matched.label[matched.run.0..matched.run.1],
                    "[native2-late]"
                );
                assert!(
                    prepared.wash_span(at_chevron, false).is_some(),
                    "the run draws its wash"
                );
            }
        }
    });
}

#[gpui::test]
fn i50_fit_columns_glyphs_stay_inside_the_column(cx: &mut TestAppContext) {
    let window = cx.add_empty_window();
    window.update(|window, _| {
        let font = font("JetBrains Mono");
        let advance = f32::from(super::shape("0", &font, px(14.), window).line().width);
        assert!(advance > 0., "the test face shapes a digit");
        let shaping = Shaping {
            font: font.clone(),
            body: px(14.),
            caption: px(12.),
            plain_cells: 20,
            chevron_cells: 18,
            clip_cells: 0,
            size: false,
            languages: false,
            scale: 1.,
            words: Default::default(),
            locating: false,
            query: "".into(),
        };
        let mut cache = TrackCache::default();
        cache.begin(&shaping, 3);
        for (index, name) in [
            "\u{898B}\u{7A4D}\u{66F8}\u{3068}\u{8B70}\u{4E8B}\u{9332}\u{3068}\u{6700}\u{7D42}\u{7248}\u{306E}\u{8CC7}\u{6599}_2025.pdf",
            "\u{0E17}\u{0E35}\u{0E48}\u{0E2A}\u{0E38}\u{0E14}\u{0E02}\u{0E2D}\u{0E07}\u{0E1B}\u{0E35}\u{0E19}\u{0E35}\u{0E49}-\u{0E23}\u{0E39}\u{0E1B}\u{0E20}\u{0E32}\u{0E1E}.png",
            "screenshot-2026-08-30-final-review-v3.png",
        ]
        .into_iter()
        .enumerate()
        {
            let facts = RowFacts {
                name: name.into(),
                ..RowFacts::default()
            };
            let prepared = cache.get(index, Row::Listing(&facts), false, None, &shaping, window);
            let width =
                |text: &str| f32::from(super::shape(text, &font, px(14.), window).line().width);
            // The widest middle elision whose shaped glyphs fit 20 cells, found by walking every budget.
            let widest = (0..=crate::row::cell_count(name))
                .rev()
                .map(|cells| crate::row::middle_elide(name, cells))
                .find(|text| width(text) <= 20. * advance + 0.01)
                .unwrap();
            assert_eq!(prepared.label, widest, "{name:?} at {advance}px a cell");
            // An ideograph is narrower than its two cells, so the glyphs keep more than the cells would.
            if index == 0 {
                assert!(
                    prepared.label.chars().count()
                        > crate::row::middle_elide(name, 20).chars().count(),
                    "{:?} is cut where its cells are",
                    prepared.label
                );
            }
            // In the monospace face a Latin name is cut exactly where the cells cut it.
            if name.is_ascii() {
                assert_eq!(prepared.label, crate::row::middle_elide(name, 20));
            }
        }
    });
}
