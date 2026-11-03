// A small raster draws at its own pixels, one to each device pixel and centred; only zoom enlarges it.
fn own_size_sources(dir: &Path) -> Vec<RowFacts> {
    image::RgbaImage::from_fn(64, 64, |x, y| {
        image::Rgba(if (x + y) % 2 == 0 {
            [0, 0, 0, 255]
        } else {
            [255, 255, 255, 255]
        })
    })
    .save(dir.join("art.png"))
    .unwrap();
    image::RgbaImage::from_fn(320, 240, |x, y| {
        image::Rgba([x as u8, y as u8, (x ^ y) as u8, 255])
    })
    .save(dir.join("photo.png"))
    .unwrap();
    vec![file("art.png", "image-png"), file("photo.png", "image-png")]
}

#[gpui::test]
fn preview_quality_look_small_rasters_stay_at_their_own_pixels(cx: &mut TestAppContext) {
    let _watchdog = watchdog(300);
    cx.executor().allow_parking();
    let root = root();
    let (shell, cx, mut requests) = opened_at(cx, &root.0);
    land(
        &shell,
        cx,
        &root.0.to_string_lossy(),
        own_size_sources(&root.0),
        1,
    );
    for (index, (w, h)) in [(0, (64u32, 64u32)), (1, (320, 240))] {
        cx.simulate_scale_factor_change(1.25);
        draw(cx);
        open_picture(&shell, cx, &mut requests, index, w, h);
        let look = look(&shell, cx);
        let source = image::open(root.0.join(["art.png", "photo.png"][index]))
            .unwrap()
            .into_rgba8();
        for scale in [1.25f32, 1.6, 2.] {
            cx.simulate_scale_factor_change(scale);
            let raster = wait(cx, "the picture at its own pixels", |cx| {
                look.read_with(cx, |look, _| {
                    let (size, ready) = look.quality_pixels()?;
                    let raster = look.svg_raster()?;
                    let drawn = (
                        raster.size(0).width.0 as u32,
                        raster.size(0).height.0 as u32,
                    );
                    (ready && drawn == size).then_some(raster)
                })
            });
            let drawn = (
                raster.size(0).width.0 as u32,
                raster.size(0).height.0 as u32,
            );
            assert_eq!(
                drawn,
                (w, h),
                "row {index} decodes its own pixels at {scale}"
            );
            assert!(
                raster
                    .as_bytes(0)
                    .unwrap()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .copied()
                    .zip(source.pixels())
                    .all(|(bgra, own)| bgra == [own[2], own[1], own[0], own[3]]),
                "row {index} keeps every source pixel at {scale}"
            );
            let (rect, card) = (picture_rect(cx), cx.debug_bounds("pv1-card").unwrap());
            for (side, want) in [(rect.size.width, w), (rect.size.height, h)] {
                let pixels = f32::from(side) * scale;
                assert!(
                    pixels <= want as f32 + 1e-3 && pixels > want as f32 - 0.01,
                    "row {index} draws {pixels} device pixels for {want} at {scale}"
                );
            }
            let centre = |b: Bounds<Pixels>| f32::from(b.origin.x) + f32::from(b.size.width) / 2.;
            assert!(
                (centre(rect) - centre(card)).abs() <= 1.,
                "row {index} is centred at {scale}: {rect:?} in {card:?}"
            );
        }
        cx.simulate_keystrokes("escape");
        draw(cx);
    }
    close(&shell, cx);
}
