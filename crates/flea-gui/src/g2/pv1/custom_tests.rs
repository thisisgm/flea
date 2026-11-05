use super::{
    custom::{Custom, Item, Options},
    custom_chrome::band,
    picture::pinch_zoom,
};
use flea::{
    jsondoc::{self, Json},
    protocol::RowFacts,
};
use gpui::{px, size};

fn custom() -> Custom {
    let mut custom = Custom::new(Options::read(&Json::Null));
    custom.items = (0..20)
        .map(|i| Item {
            path: format!("/files/{i}.png"),
            row: RowFacts::default(),
        })
        .collect();
    custom
}
#[test]
fn custom_port_filmstrip_contains_only_whole_cards_at_both_ends() {
    for width in [480., 640., 853.3, 1024.5, 1920.] {
        for current in 0..20 {
            let range = band(20, current, width);
            assert!(range.contains(&current));
            assert!(range.end <= 20);
            assert!(range.len() as f32 * 92. <= width - 24.);
        }
    }
}
#[test]
fn custom_port_toolbar_hiding_cannot_shift_fullscreen_content() {
    let mut custom = custom();
    custom.fullscreen = true;
    let before = custom.card(size(px(1000.), px(700.)));
    custom.navbar = false;
    assert_eq!(custom.card(size(px(1000.), px(700.))), before);
    assert!(custom.filmstrip_visible());
    assert_eq!(custom.card(size(px(1000.), px(700.))).size.height, px(608.));
}
#[test]
fn custom_port_settings_restore_the_saved_choices() {
    let doc = jsondoc::parse(r#"{"preview":{"hideFilmstrip":true,"infoPanel":true,"preloadAdjacent":false,"mediaControls":false,"navbarHideMs":6000}}"#).unwrap();
    let options = Options::read(&doc);
    assert!(!options.preload && !options.media_controls);
    assert!(options.info);
    assert_eq!(options.delay, 6000);
    let items = custom().items;
    let mut custom = Custom::new(options);
    custom.items = items;
    assert!(custom.filmstrip_visible());
}
#[test]
fn custom_port_info_cannot_remove_the_content_viewport() {
    let mut custom = custom();
    custom.info = true;
    for (w, h) in [(480., 360.), (900., 600.), (120., 80.)] {
        let bounds = custom.card(size(px(w), px(h)));
        assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
    }
}

#[test]
fn quick_look_pinch_zoom_is_smooth_and_bounded() {
    assert!((pinch_zoom(1., 0.1) - 1.1).abs() < 0.0001);
    assert!((pinch_zoom(2., -0.25) - 1.5).abs() < 0.0001);
    assert_eq!(pinch_zoom(3.9, 0.5), 4.);
    assert_eq!(pinch_zoom(1.1, -2.), 1.);
}
