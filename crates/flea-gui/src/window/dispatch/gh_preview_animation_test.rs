#[gpui::test]
fn gh336_scaled_gif_keeps_its_animation_frames(cx: &mut TestAppContext) {
    let _watchdog = watchdog(300);
    cx.executor().allow_parking();
    let root = root();
    let path = root.0.join("moving.gif");
    {
        let mut encoder =
            image::codecs::gif::GifEncoder::new(std::fs::File::create(&path).unwrap());
        encoder
            .set_repeat(image::codecs::gif::Repeat::Infinite)
            .unwrap();
        for color in [[220, 20, 40, 255], [20, 220, 40, 255]] {
            encoder
                .encode_frame(image::Frame::from_parts(
                    image::RgbaImage::from_pixel(800, 600, image::Rgba(color)),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(100, 1),
                ))
                .unwrap();
        }
    }
    let (shell, cx, mut requests) = opened_at(cx, &root.0);
    land(
        &shell,
        cx,
        &root.0.to_string_lossy(),
        vec![file("moving.gif", "image-gif")],
        1,
    );
    open_picture(&shell, cx, &mut requests, 0, 800, 600);
    let look = look(&shell, cx);
    let image = wait(cx, "the animated picture", |cx| {
        look.read_with(cx, |look, _| look.svg_raster())
    });
    assert_eq!(
        image.frame_count(),
        2,
        "scaled images preserve GIF animation"
    );
    assert_eq!((image.size(0).width.0, image.size(0).height.0), (800, 600));
    close(&shell, cx);
}
