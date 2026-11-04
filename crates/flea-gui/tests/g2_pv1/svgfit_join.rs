use super::{boards::rect, pixels, scenes, *};

#[gpui::test]
fn raw_picture_fit_keeps_caption_space_for_vectors_and_rasters(cx: &mut TestAppContext) {
    for vector in [true, false] {
        let mut subject = scenes::ql_raw(&crate::test_catalogs::english());
        let picture = if vector {
            scenes::ql_svg()
        } else {
            scenes::ql_image()
        };
        if let (Body::Raw { path, w, h, .. }, Body::Image { path: source, .. }) =
            (&mut subject.body, picture.body)
        {
            *path = source;
            *w = 60.;
            *h = 40.;
        } else {
            panic!("raw and image fixtures");
        }
        subject.shape.w = 60.;
        subject.shape.h = 40.;
        let (cx, _, _, _) = host(cx, subject, Arm::Fix);
        let (_, _, width, height) = rect(cx, "pv1-picture");
        let (_, _, _, card_height) = rect(cx, "pv1-card");
        let (_, _, _, caption_height) = rect(cx, "pv1-caption");
        if vector {
            assert_eq!(height + caption_height, card_height);
            assert!((width / height - 1.5).abs() < 0.01);
        } else {
            pixels::assert_own_pixels(cx, "pv1-picture", (60., 40.));
        }
    }
}
