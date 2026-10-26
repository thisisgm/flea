#[gpui::test]
fn gh336_zoom_keeps_letterboxed_picture_centred_and_preserves_a_panned_anchor(
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
    open_picture(&shell, cx, &mut requests, 1, 300, 600);
    let look = look(&shell, cx);
    raster_other_than(cx, &look, None);
    let initial = picture_rect(cx);
    for _ in 0..12 {
        look.update(cx, |look, cx| {
            look.action("zoomIn", cx);
        });
        draw(cx);
        let rect = picture_rect(cx);
        let view = look.read_with(cx, |look, _| look.scroll.view.unwrap());
        let centre = rect.origin.x + rect.size.width / 2.;
        let wanted = view.bounds.origin.x + view.bounds.size.width / 2.;
        assert!(
            (f32::from(centre) - f32::from(wanted)).abs() < 0.6,
            "crossing letterbox overflow stays centred"
        );
    }
    look.update(cx, |look, _| {
        look.scroll.x += 20.;
        look.scroll.y += 30.;
    });
    draw(cx);
    let (old, x, y) = look.read_with(cx, |look, _| {
        (look.scroll.view.unwrap(), look.scroll.x, look.scroll.y)
    });
    look.update(cx, |look, cx| {
        look.action("zoomOut", cx);
    });
    draw(cx);
    let (new, nx, ny) = look.read_with(cx, |look, _| {
        (look.scroll.view.unwrap(), look.scroll.x, look.scroll.y)
    });
    let f = |p: Pixels| f64::from(f32::from(p));
    let anchor_x = (x + f(old.bounds.size.width) / 2.) / old.content_w;
    let anchor_y = (y + f(old.bounds.size.height) / 2.) / old.content_h;
    assert!(((nx + f(new.bounds.size.width) / 2.) / new.content_w - anchor_x).abs() < 0.001);
    assert!(((ny + f(new.bounds.size.height) / 2.) / new.content_h - anchor_y).abs() < 0.001);
    look.update(cx, |look, cx| {
        look.action("zoomOut", cx);
        look.action("zoomIn", cx);
    });
    draw(cx);
    let (bx, by) = look.read_with(cx, |look, _| (look.scroll.x, look.scroll.y));
    assert!(
        (bx - nx).abs() < 0.01 && (by - ny).abs() < 0.01,
        "batched zoom keeps the panned anchor"
    );
    look.update(cx, |look, cx| {
        look.action("zoomFit", cx);
    });
    draw(cx);
    assert_eq!(picture_rect(cx), initial);
    close(&shell, cx);
}
