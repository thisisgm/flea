use super::{
    composition::*,
    geometry::Composition,
    output::{self, Kind},
    pages::Pages,
    selection::*,
};

fn selected(kind: Kind, names: &[&str]) -> Selection {
    Selection {
        generation: 7,
        menu: 3,
        files: names
            .iter()
            .map(|name| Source {
                path: format!("/photos/{name}"),
                kind,
            })
            .collect(),
    }
}

#[test]
fn op4_convert_choices_names_and_sequence_match_the_oracle() {
    let s = selected(Kind::Image, &["photo.png", "other.jpg"]);
    assert_eq!(default_format(&s.files[0]), "jpg");
    assert_eq!(
        output::IMAGE_FORMATS,
        ["jpg", "png", "webp", "avif", "heic", "tiff", "bmp", "pdf"]
    );
    assert_eq!(
        s.output_paths("png", false, &Pages::All, None).unwrap(),
        [
            "/photos/photo (stripped).png",
            "/photos/other (converted).png"
        ]
        .map(std::path::PathBuf::from)
    );
    assert_eq!(
        s.output_paths("pdf", true, &Pages::All, None).unwrap(),
        [std::path::PathBuf::from("/photos/photo (converted).pdf")]
    );
    assert_eq!(
        s.output_paths("pdf", false, &Pages::All, None)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        selected(Kind::Pdf, &["report.pdf"])
            .output_paths("png", false, &Pages::parse("3,1").unwrap(), Some(3))
            .unwrap(),
        ["/photos/report (page 1).png", "/photos/report (page 3).png"]
            .map(std::path::PathBuf::from)
    );
}

#[test]
fn op4_invalid_selection_ranges_and_duplicate_outputs_refuse() {
    assert!(selected(Kind::Image, &[]).validate().is_err());
    assert!(selected(Kind::Image, &["a.png", "a.png"])
        .validate()
        .is_err());
    assert!(selected(Kind::Image, &["a.png", "a.jpg"])
        .output_paths("webp", false, &Pages::All, None)
        .is_err());
    let mut s = selected(Kind::Image, &["a.png", "b.png"]);
    s.files[1].path = "/another/b.png".into();
    assert!(s.output_paths("pdf", true, &Pages::All, None).is_err());
    s.files[0].path = "relative.png".into();
    assert!(s.validate().is_err());
    assert!(selected(Kind::Pdf, &["a.pdf"])
        .output_paths("png", false, &Pages::parse("4").unwrap(), Some(3))
        .is_err());
}

#[test]
fn op4_preview_and_commands_share_exact_gutters_and_bankers_rounding() {
    let mut s = Settings {
        layout: Layout::Row,
        spacing: Spacing::Small,
        background: Background::Transparent,
        columns: 2,
    };
    assert_eq!(Spacing::Small.pixels(150), 4);
    assert_eq!(Spacing::Small.pixels(50), 2);
    let row = Composition::new(&[(100, 100); 2], &s).unwrap();
    assert_eq!((row.width, row.height), (209., 106.));
    assert_eq!(
        row.tiles.iter().map(|r| (r.x, r.y)).collect::<Vec<_>>(),
        [(3., 3.), (106., 3.)]
    );
    s.layout = Layout::Grid;
    let grid = Composition::new(&[(100, 100); 4], &s).unwrap();
    assert_eq!((grid.width, grid.height), (209., 209.));
    assert_eq!(
        grid.tiles.iter().map(|r| (r.x, r.y)).collect::<Vec<_>>(),
        [(3., 3.), (106., 3.), (3., 106.), (106., 106.)]
    );
    assert_eq!(
        join_commands(
            &["/a.png".into(), "/b.png".into()],
            std::path::Path::new("/out.png"),
            (100, 100),
            &s
        )
        .len(),
        2
    );
    s.layout = Layout::Column;
    let column = Composition::new(&[(100, 100); 2], &s).unwrap();
    assert_eq!((column.width, column.height), (106., 209.));
    assert!(Composition::new(&[(0, 100); 2], &s).is_err());
    assert!(Composition::new(&[(1, 50_001); 2], &s).is_err());
}

#[test]
fn op4_thin_sources_keep_a_hittable_pixel_in_every_fitted_layout() {
    let mut vanished = Vec::new();
    for layout in [Layout::Row, Layout::Column, Layout::Grid] {
        let settings = Settings {
            layout,
            spacing: Spacing::None,
            background: Background::Transparent,
            columns: 2,
        };
        let preview = Composition::new(&[(1, 10_000), (10_000, 1)], &settings)
            .unwrap()
            .fitted(516., 278., 14.);
        if preview
            .tiles
            .iter()
            .any(|tile| tile.width < 1. || tile.height < 1.)
        {
            vanished.push(layout);
        }
    }
    assert!(
        vanished.is_empty(),
        "thin sources became unhittable after fitting: {vanished:?}"
    );
}

#[test]
fn op4_option_validation_keeps_transparency_and_background_eligibility() {
    assert!(!background_enabled(Layout::Grid, Spacing::None, 4, 2));
    assert!(background_enabled(Layout::Grid, Spacing::None, 3, 2));
    assert!(Settings::parse("Grid", "Small", "Transparent", "jpg", 2).is_err());
    assert!(Settings::parse("Grid", "Small", "Transparent", "png", 0).is_err());
    assert!(Settings::parse("Row", "Large", "Black", "webp", 2).is_ok());
}
