//! #336 image request regressions through the production Shell and real files.
include!("gh_preview_fixture.rs");

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="300" viewBox="0 0 400 300"><rect width="400" height="300" fill="#2a6"/><circle cx="200" cy="150" r="90" fill="#fff"/></svg>"##;

fn picture_rect(cx: &mut VisualTestContext) -> Bounds<Pixels> {
    cx.debug_bounds("pv1-picture")
        .expect("the card draws its picture")
}

fn physical(rect: Bounds<Pixels>, scale: f64) -> (i32, i32) {
    (
        (f64::from(f32::from(rect.size.width)) * scale).ceil() as i32,
        (f64::from(f32::from(rect.size.height)) * scale).ceil() as i32,
    )
}

fn raster_other_than(
    cx: &mut VisualTestContext,
    look: &Entity<QuickLook>,
    not: Option<usize>,
) -> Arc<RenderImage> {
    wait(cx, "a new SVG raster", |cx| {
        look.read_with(cx, |look, _| look.svg_raster())
            .filter(|image| Some(image.id.0) != not)
    })
}

fn image_listing(dir: &Path) -> Vec<RowFacts> {
    std::fs::write(dir.join("pic.svg"), SVG).unwrap();
    png(&dir.join("big.png"), 4000, 3000);
    png(&dir.join("small.png"), 800, 600);
    vec![
        file("big.png", "image-png"),
        file("pic.svg", "image-svg+xml"),
        file("small.png", "image-png"),
    ]
}

fn open_picture(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    requests: &mut super::preview_tests::Lines,
    index: usize,
    w: u32,
    h: u32,
) {
    set_cursor(shell, cx, index);
    cx.simulate_keystrokes("space");
    draw(cx);
    let meta = wait(cx, "the meta ask", |_| {
        Some(sent(requests, "meta")).filter(|a| !a.is_empty())
    });
    assert_eq!(meta.len(), 1, "one meta ask for row {index}");
    answer(shell, cx, meta_reply(index, w, h, token(&meta[0])));
    assert!(
        matches!(body(shell, cx), Some(Body::Image { .. })),
        "row {index} mounts the picture body"
    );
}

#[gpui::test]
fn gh336_space_svg_rasterizes_at_physical_pixels_at_scale_2_and_1_and_on_each_scale_change(
    cx: &mut TestAppContext,
) {
    let _watchdog = watchdog(300);
    cx.executor().allow_parking();
    let root = root();
    let (shell, cx, mut requests) = opened_at(cx, &root.0);
    land(
        &shell,
        cx,
        &root.0.to_string_lossy(),
        image_listing(&root.0),
        1,
    );
    open_picture(&shell, cx, &mut requests, 1, 400, 300);
    let look = look(&shell, cx);
    assert_eq!(
        cx.update(|window, _| window.scale_factor()),
        2.0,
        "the test window starts at scale 2"
    );
    let first = raster_other_than(cx, &look, None);
    let rect = picture_rect(cx);
    let s = first.size(0);
    eprintln!(
        "gh336 svg scale2 fit={rect:?} raster={}x{}",
        s.width.0, s.height.0
    );
    assert_eq!(
        (s.width.0, s.height.0),
        physical(rect, 2.),
        "scale 2: the raster is twice the logical fit"
    );
    let mut prev = first.id.0;
    for scale in [1.0f32, 2.0, 1.5, 1.0] {
        cx.simulate_scale_factor_change(scale);
        draw(cx);
        let next = raster_other_than(cx, &look, Some(prev));
        let now = picture_rect(cx);
        let s = next.size(0);
        eprintln!(
            "gh336 svg scale{scale} fit={now:?} raster={}x{}",
            s.width.0, s.height.0
        );
        // Painted bounds align to physical pixels; the logical fit still owns the decode request.
        for (painted, logical) in [
            (f32::from(now.size.width), f32::from(rect.size.width)),
            (f32::from(now.size.height), f32::from(rect.size.height)),
        ] {
            assert!((painted - logical).abs() < 1. / scale);
        }
        // The painted fit covers the raster's own device pixels, so the raster draws one to one.
        let (pw, ph) = physical(rect, scale.into());
        assert_eq!(
            now.size,
            size(px(pw as f32 / scale), px(ph as f32 / scale)),
            "scale {scale}: the painted fit lands on the raster's device pixels"
        );
        assert_eq!(
            (s.width.0, s.height.0),
            physical(rect, scale.into()),
            "scale {scale}: a new raster at the logical fit's physical pixels"
        );
        prev = next.id.0;
    }
    close(&shell, cx);
}

#[gpui::test]
fn gh336_space_svg_zoomed_2x_at_scale_2_rasterizes_at_4x_its_fit(cx: &mut TestAppContext) {
    let _watchdog = watchdog(300);
    cx.executor().allow_parking();
    let root = root();
    let (shell, cx, mut requests) = opened_at(cx, &root.0);
    land(
        &shell,
        cx,
        &root.0.to_string_lossy(),
        image_listing(&root.0),
        1,
    );
    open_picture(&shell, cx, &mut requests, 1, 400, 300);
    let look = look(&shell, cx);
    let first = raster_other_than(cx, &look, None);
    let rect = picture_rect(cx);
    // ZOOM_STEP is 0.25, so four zoomIn presses take the picture from fit to 2x, as v0.3.12 PreviewImage zoomBy does.
    let zoom_in = flea::keys::binding_rows("default", flea::keys::Frontend::Gui)
        .find(|binding| binding.action == "zoomIn")
        .map(keystroke)
        .expect("default binds zoomIn");
    for step in 1..=4 {
        cx.simulate_event(zoom_in.clone());
        draw(cx);
        let want = physical(rect, 2. * (1. + step as f64 * 0.25));
        wait(cx, "the SVG raster at this zoom step", |cx| {
            look.read_with(cx, |look, _| look.svg_raster())
                .filter(|i| (i.size(0).width.0, i.size(0).height.0) == want)
        });
        assert_eq!(
            read(&shell, cx, "previewOpen", &[]),
            "true",
            "zoomIn keeps the card open"
        );
    }
    let start = Instant::now();
    let mut seen = None;
    while start.elapsed() < Duration::from_secs(10) {
        cx.executor().advance_clock(Duration::from_millis(50));
        draw(cx);
        seen = look
            .read_with(cx, |look, _| look.svg_raster())
            .filter(|i| i.id.0 != first.id.0);
        if seen.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let want = physical(rect, 4.);
    let got = seen
        .as_ref()
        .map(|i| (i.size(0).width.0, i.size(0).height.0));
    eprintln!(
        "gh336 svg zoom fit={rect:?} want={want:?} got={got:?} first={}x{}",
        first.size(0).width.0,
        first.size(0).height.0
    );
    assert_eq!(
        got,
        Some(want),
        "MISSING-PRODUCER: a 2x zoom at scale 2 redraws the SVG at 4x its logical fit"
    );
    close(&shell, cx);
}

#[gpui::test]
fn gh336_space_raster_pictures_decode_at_the_fit_times_scale_bounded_by_the_source(
    cx: &mut TestAppContext,
) {
    let _watchdog = watchdog(300);
    cx.executor().allow_parking();
    let root = root();
    let (shell, cx, mut requests) = opened_at(cx, &root.0);
    land(
        &shell,
        cx,
        &root.0.to_string_lossy(),
        image_listing(&root.0),
        1,
    );
    let mut verdicts = Vec::new();
    for (index, name, w, h) in [(0, "big.png", 4000u32, 3000u32), (2, "small.png", 800, 600)] {
        for scale in [2.0f32, 1.0] {
            cx.simulate_scale_factor_change(scale);
            draw(cx);
            open_picture(&shell, cx, &mut requests, index, w, h);
            let path = root.0.join(name);
            let image = wait(cx, "the picture decodes", |cx| decoded(cx, &shell, &path));
            let rect = picture_rect(cx);
            let want = flea::ui_logic_preview_core::pixels::decode_size(
                f64::from(f32::from(rect.size.width)),
                f64::from(f32::from(rect.size.height)),
                scale.into(),
                w.into(),
                h.into(),
                false,
            );
            let got = (image.size(0).width.0 as u32, image.size(0).height.0 as u32);
            eprintln!("gh336 raster {name} scale{scale} fit={rect:?} spec_request={want:?} decoded={got:?}");
            verdicts.push((name, scale, want, got));
            cx.simulate_keystrokes("escape");
            draw(cx);
            cx.update(|_, cx| {
                cx.remove_asset::<ImgResourceLoader>(&Resource::Path(Arc::from(path.as_path())))
            });
        }
    }
    let wrong: Vec<_> = verdicts
        .iter()
        .filter(|(_, _, want, got)| want != got)
        .collect();
    assert!(
        wrong.is_empty(),
        "decode not at fit x scale bounded by source: {wrong:?}"
    );
    close(&shell, cx);
}

#[gpui::test]
fn gh336_raster_zoom_requests_larger_pixels_retains_on_zoom_out_and_releases_at_fit(
    cx: &mut TestAppContext,
) {
    let _watchdog = watchdog(300);
    cx.executor().allow_parking();
    let root = root();
    let (shell, cx, mut requests) = opened_at(cx, &root.0);
    land(
        &shell,
        cx,
        &root.0.to_string_lossy(),
        image_listing(&root.0),
        1,
    );
    open_picture(&shell, cx, &mut requests, 0, 4000, 3000);
    let look = look(&shell, cx);
    let first = wait(cx, "the fit photo", |cx| {
        look.read_with(cx, |look, _| look.svg_raster())
    });
    let rect = picture_rect(cx);
    let zoom_in = flea::keys::binding_rows("default", flea::keys::Frontend::Gui)
        .find(|binding| binding.action == "zoomIn")
        .map(keystroke)
        .unwrap();
    for step in 1..=12 {
        cx.simulate_event(zoom_in.clone());
        draw(cx);
        let zoom = 1. + step as f64 * 0.25;
        let want = flea::ui_logic_preview_core::pixels::decode_size(
            f64::from(f32::from(rect.size.width)) * zoom,
            f64::from(f32::from(rect.size.height)) * zoom,
            2.,
            4000.,
            3000.,
            false,
        );
        wait(cx, "the photo at this zoom step", |cx| {
            look.read_with(cx, |look, _| look.svg_raster())
                .filter(|i| (i.size(0).width.0 as u32, i.size(0).height.0 as u32) == want)
        });
    }
    let full = wait(cx, "the final sharp decode at full zoom", |cx| {
        look.read_with(cx, |look, _| {
            look.svg_raster()
                .filter(|_| look.quality_pixels().is_some_and(|(_, ready)| ready))
        })
    });
    cx.simulate_event(zoom_in);
    draw(cx);
    assert_eq!(
        look.read_with(cx, |look, _| look.svg_raster()).unwrap().id,
        full.id,
        "4x is the ceiling"
    );
    look.update(cx, |look, cx| {
        look.action("zoomOut", cx);
    });
    draw(cx);
    assert_eq!(
        look.read_with(cx, |look, _| look.svg_raster()).unwrap().id,
        full.id,
        "zoom out reuses the sharp decode"
    );
    look.update(cx, |look, cx| {
        look.action("zoomFit", cx);
    });
    draw(cx);
    wait(cx, "fit retires the large decode", |cx| {
        look.read_with(cx, |look, _| look.svg_raster())
            .filter(|i| i.size(0) == first.size(0))
    });
    assert_eq!(
        picture_rect(cx).size,
        rect.size,
        "fit restores logical geometry"
    );
    close(&shell, cx);
}

include!("gh_preview_animation_test.rs");

include!("gh_preview_zoom_test.rs");
include!("gh_preview_own_size_test.rs");
include!("fixpreview_zoom_test.rs");

include!("mdlook_tests.rs");

#[gpui::test]
fn preview_quality_look_picture_refines_in_place_at_its_device_pixels(cx: &mut TestAppContext) {
    let _watchdog = watchdog(300);
    cx.executor().allow_parking();
    let root = root();
    let (shell, cx, mut requests) = opened_at(cx, &root.0);
    land(
        &shell,
        cx,
        &root.0.to_string_lossy(),
        image_listing(&root.0),
        1,
    );
    cx.simulate_scale_factor_change(1.5);
    draw(cx);
    open_picture(&shell, cx, &mut requests, 0, 4000, 3000);
    let look = look(&shell, cx);
    let size = wait(cx, "the refined picture", |cx| {
        look.read_with(cx, |look, _| {
            let (size, ready) = look.quality_pixels()?;
            let raster = look.svg_raster()?;
            let drawn = (
                raster.size(0).width.0 as u32,
                raster.size(0).height.0 as u32,
            );
            (ready && drawn == size).then_some(size)
        })
    });
    let want = physical(picture_rect(cx), 1.5);
    assert!(
        (size.0 as i32 - want.0).abs() <= 1 && (size.1 as i32 - want.1).abs() <= 1,
        "refined {size:?} covers the card picture's device pixels {want:?}"
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    close(&shell, cx);
}
