use super::*;

// A figure's natural size, decoded from an SVG that names 30 by 18 logical pixels at a window scale.
fn figure_size(scale: f32) -> (f32, f32) {
    let decoder = Decoder::spawn(SvgRenderer::new(Arc::new(())));
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="30" height="18"><rect width="30" height="18"/></svg>"#;
    decoder.svg(1, Art::Figure(0), svg.into(), scale);
    match decoder
        .decoded
        .recv_blocking()
        .expect("the decoder answers")
    {
        Decoded::Svg {
            image: Some((_, natural)),
            ..
        } => natural,
        _ => panic!("the figure decodes"),
    }
}

#[test]
fn an_svg_figure_keeps_its_own_size_at_every_window_scale() {
    // GPUI rasterises an SVG at twice the asked scale, so dividing by the window scale alone doubled every diagram (EY-41).
    assert_eq!(figure_size(1.), (30., 18.));
    assert_eq!(figure_size(2.), (30., 18.));
}

#[test]
fn a_figure_with_an_open_ticket_is_asked() {
    let mut doc = super::super::doc::Doc::default();
    assert!(!doc.figure_asked(2));
    doc.figure_tickets.insert(7, 2);
    assert!(doc.figure_asked(2));
    assert!(!doc.figure_asked(3));
}
