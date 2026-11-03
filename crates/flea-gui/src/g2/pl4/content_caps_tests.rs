//! I50-26: rail headings take the language's own capitals, and neither capitals nor tracking in a caseless language.
use super::{build, heading_text};
use crate::row::fit_tests::catalog;
use flea::ui_logic_pl4::{
    order::{Heading, RailSection},
    tagtheme::TagTheme,
};

#[test]
fn i50_fit_rail_heading_caps() {
    for (language, key, heading) in [
        ("tr", "places.heading.tags", "ET\u{130}KETLER"),
        (
            "el",
            "places.heading.places",
            "\u{3a4}\u{39f}\u{3a0}\u{39f}\u{398}\u{395}\u{3a3}\u{399}\u{395}\u{3a3}",
        ),
        ("en", "places.heading.places", "PLACES"),
    ] {
        let catalog = catalog(language);
        assert_eq!(
            heading_text(&catalog, &Heading::Key(key)).to_string(),
            heading,
            "{language}"
        );
    }
    let german = catalog("de");
    assert_eq!(
        heading_text(&german, &Heading::Plugin("Gr\u{f6}\u{df}e".into())).to_string(),
        "GR\u{d6}SSE"
    );
    assert!(heading_text(&german, &Heading::None).is_empty());
}

#[test]
fn i50_fit_rail_heading_caseless() {
    let key = "places.heading.places";
    let sections = [RailSection {
        heading: Heading::Key(key),
        rows: Vec::new(),
    }];
    for language in ["ja", "zh-CN", "ko", "ar", "fa", "he", "ur", "th"] {
        let catalog = catalog(language);
        assert_eq!(
            heading_text(&catalog, &Heading::Key(key)).to_string(),
            catalog.tr(key),
            "{language}"
        );
        // A Latin plugin title keeps its own case too.
        assert_eq!(
            heading_text(&catalog, &Heading::Plugin("Photos".into())).to_string(),
            "Photos",
            "{language}"
        );
        // No capitals, so no tracking either.
        assert!(
            build(&catalog, &sections, &TagTheme::default())[0].caseless,
            "{language}"
        );
    }
    assert!(!build(&catalog("de"), &sections, &TagTheme::default())[0].caseless);
}
