use super::*;

#[test]
fn drawing_keeps_columns_and_lines_in_their_named_dimensions() {
    let mut model = Model::new(std::path::PathBuf::from("/"), &crate::jsondoc::Json::Null);
    model.scrolled_at = None;
    assert!(!model.find.waiting && !model.find.cancelling);
    let theme = Theme::from_text("");
    let map = Map::load();
    let elapsed = std::time::Duration::ZERO;
    let expected = crate::tui::look::frame(&model, &theme, &map, 80, 24, elapsed);
    assert_ne!(
        expected,
        crate::tui::look::frame(&model, &theme, &map, 24, 80, elapsed)
    );
    let mut last = expected.clone();
    assert!(!draw(
        &model,
        &theme,
        &map,
        TerminalSize {
            columns: 80,
            lines: 24
        },
        elapsed,
        &mut last,
    )
    .unwrap());
    assert_eq!(last, expected);
}
