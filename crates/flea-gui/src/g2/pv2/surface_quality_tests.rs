use super::{
    tests::{land, opened, renders, LANDSCAPE},
    *,
};

#[test]
fn preview_quality_pdf_requests_cover_fractional_device_rects() {
    for continuous in [false, true] {
        let mut surface = opened(
            continuous,
            "/d/page.pdf",
            1,
            vec![LANDSCAPE],
            (600.4, 900.4),
        );
        for scale in [1., 1.25, 1.5, 2.] {
            surface.set_frame(600.4, 900.4, scale, 9.);
            assert_eq!(
                surface.render_width,
                (600.4 * scale).ceil() as u32,
                "every device pixel is covered"
            );
        }
    }
}

#[test]
fn preview_quality_pdf_small_resize_refines_without_dropping_the_page() {
    for continuous in [false, true] {
        let mut surface = opened(continuous, "/d/page.pdf", 1, vec![LANDSCAPE], (600., 900.));
        surface.take_commands();
        land(&mut surface, 1, 0, 600, 1);
        let old = surface.image(0).unwrap().clone();
        surface.set_frame(630., 900., 1., 9.);
        assert!(
            renders(&surface.take_commands()).contains(&(0, 630)),
            "a five percent growth must render again"
        );
        assert!(
            Arc::ptr_eq(surface.image(0).unwrap(), &old),
            "the previous page stays until refinement"
        );
        land(&mut surface, 1, 0, 630, 2);
        assert!(
            !Arc::ptr_eq(surface.image(0).unwrap(), &old),
            "the exact raster replaces it"
        );
    }
}
