//! EY-437: Columns draws a late filter run whole inside the name's cells, at the plain and the chevron budget.
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
