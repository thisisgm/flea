use super::tests::Fixture;
use crate::tui::{cells, layout, render::frame_tests::plain, theme::Theme};
use std::time::Duration;

#[test]
fn ar_tui_copy_real_footer_keeps_few_noun_and_complete_shortcut_word() {
    let f = Fixture::new();
    f.write(
        "catalog/ar/flea.json",
        include_str!("../../i18n/ar/flea.json"),
    );
    f.write("language", "ar\n");
    let mut model = f.model();
    assert_eq!(model.i18n.language(), "ar");
    assert!(model.selected.is_empty());
    let theme = Theme::from_text("");
    for count in [0, 1, 2, 3, 9, 10, 11, 99, 100] {
        model.total = count;
        let footer = layout::footer(&model, &theme, 130, Duration::ZERO);
        let logical = cells::clean(&plain(&footer));
        let expected = cells::clean(&model.i18n.trn("items.count", count as i64));
        assert!(logical.contains(&expected), "count={count}: {logical:?}");
        if [3, 9, 10].contains(&count) {
            assert!(logical.contains("عناصر"), "count={count}: {logical:?}");
        }
        let visual = plain(&cells::visual::frame(&footer));
        assert!(visual.contains("تاراصتخالا"), "count={count}: {visual:?}");
        assert!(visual.contains('?'));
        if [3, 9, 10].contains(&count) {
            assert!(visual.contains("رصانع"), "count={count}: {visual:?}");
        }
        assert_eq!(cells::text_width(&visual), 130);
    }
}
