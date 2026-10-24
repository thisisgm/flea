use super::*;

#[test]
fn u03_double_click_requires_the_first_taps_file_identity() {
    let mut input = super::super::PaneInput::default();
    let press = |name: &str, count| Press {
        row: 2,
        listing: 1,
        name: Some(name.into()),
        event: MouseDownEvent {
            click_count: count,
            ..Default::default()
        },
        dragged: false,
    };
    assert_eq!(input.tap_count(&press("old", 1)), 1);
    assert_eq!(input.tap_count(&press("other", 2)), 0);
    assert_eq!(input.tap_count(&press("other", 3)), 1);
    assert_eq!(input.tap_count(&press("other", 4)), 2);
}
