// EY-403: PreviewImage.qml's zoom is times the picture's own size, centred in the viewport, and a drag pans it.
fn fixpreview_zoom_in(cx: &mut VisualTestContext, steps: usize) {
    // The bound key, so each step runs inside the window's own dispatch as it does natively.
    let zoom_in = flea::keys::binding_rows("default", flea::keys::Frontend::Gui)
        .find(|binding| binding.action == "zoomIn")
        .map(keystroke)
        .expect("default binds zoomIn");
    for _ in 0..steps {
        cx.simulate_event(zoom_in.clone());
        draw(cx);
    }
}

fn fixpreview_drag(cx: &mut VisualTestContext, from: gpui::Point<Pixels>, to: gpui::Point<Pixels>) {
    use gpui::{Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
    cx.simulate_event(MouseDownEvent {
        position: from,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    for step in 1..=4 {
        let t = step as f32 / 4.;
        cx.simulate_event(MouseMoveEvent {
            position: from + (to - from) * t,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
    }
    cx.simulate_event(MouseUpEvent {
        position: to,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    draw(cx);
}

#[gpui::test]
fn fixpreview_ey403_zoom_is_own_pixels_centred_and_a_drag_pans(cx: &mut TestAppContext) {
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
    for (round, scale) in [1f32, 2.].into_iter().enumerate() {
        cx.simulate_scale_factor_change(scale);
        draw(cx);
        open_picture(&shell, cx, &mut requests, 1, 320, 240);
        let look = look(&shell, cx);
        wait(cx, "the picture's first frame", |cx| {
            look.read_with(cx, |look, _| look.svg_raster())
        });
        // ZOOM_STEP is 0.25: four steps from fit are 200%, twelve are the 400% top of the ladder.
        fixpreview_zoom_in(cx, 4);
        let rect = picture_rect(cx);
        let view = look.read_with(cx, |look, _| look.scroll.view.expect("a zoomed viewport"));
        // Twice the picture's own pixels, as PreviewImage.qml draws implicitWidth * fit * zoom for a picture that fits.
        for (side, own) in [(rect.size.width, 320.), (rect.size.height, 240.)] {
            let device = f32::from(side) * scale;
            assert!(
                (device - 2. * own).abs() <= 1.,
                "round {round}: 200% draws {device} device pixels for {own} own pixels at {scale}"
            );
        }
        let centre = |b: Bounds<Pixels>| {
            (
                f32::from(b.origin.x) + f32::from(b.size.width) / 2.,
                f32::from(b.origin.y) + f32::from(b.size.height) / 2.,
            )
        };
        let (got, want) = (centre(rect), centre(view.bounds));
        assert!(
            (got.0 - want.0).abs() <= 1. && (got.1 - want.1).abs() <= 1.,
            "round {round}: 200% is centred, {rect:?} in {:?}",
            view.bounds
        );
        // At the top of the ladder the picture overflows the viewport, and a drag on it moves it with the pointer.
        fixpreview_zoom_in(cx, 8);
        let before = picture_rect(cx);
        let view = look.read_with(cx, |look, _| look.scroll.view.unwrap());
        // Outgrowing the viewport keeps the centred point centred, as PreviewImage.qml's zoomTo does.
        let (got, want) = (centre(before), centre(view.bounds));
        assert!(
            (got.0 - want.0).abs() <= 1. && (got.1 - want.1).abs() <= 1.,
            "round {round}: 400% stays centred, {before:?} in {:?}",
            view.bounds
        );
        let at = |fx: f32, fy: f32| {
            view.bounds.origin
                + gpui::point(view.bounds.size.width * fx, view.bounds.size.height * fy)
        };
        fixpreview_drag(cx, at(0.52, 0.5), at(0.42, 0.43));
        let after = picture_rect(cx);
        let moved = after.origin - before.origin;
        let pointer = at(0.42, 0.43) - at(0.52, 0.5);
        // An axis whose content outgrows the viewport follows the pointer; one that fits holds still, as a Flickable does.
        let over = |content: f64, side: Pixels| content > f64::from(f32::from(side)) + 0.5;
        let want = gpui::point(
            if over(view.content_w, view.bounds.size.width) {
                pointer.x
            } else {
                px(0.)
            },
            if over(view.content_h, view.bounds.size.height) {
                pointer.y
            } else {
                px(0.)
            },
        );
        assert!(
            want != gpui::point(px(0.), px(0.)) || round > 0,
            "round {round}: at scale 1 the 400% picture outgrows the viewport"
        );
        assert!(
            (f32::from(moved.x) - f32::from(want.x)).abs() <= 1.
                && (f32::from(moved.y) - f32::from(want.y)).abs() <= 1.,
            "round {round}: a drag pans the picture with the pointer, moved {moved:?} for {want:?}"
        );
        cx.simulate_keystrokes("escape");
        draw(cx);
    }
    close(&shell, cx);
}

/// EY-521: PreviewImage.qml draws the picture as soon as it decodes, so a held meta reply leaves it drawn at its own size.
#[gpui::test]
fn fixpreview_ey521_a_held_meta_reply_still_draws_the_picture(cx: &mut TestAppContext) {
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
    cx.simulate_scale_factor_change(1.);
    draw(cx);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("space");
    draw(cx);
    let meta = wait(cx, "the meta ask", |_| {
        Some(sent(&mut requests, "meta")).filter(|a| !a.is_empty())
    });
    assert_eq!(meta.len(), 1, "one meta ask, left unanswered");
    let rect = wait(cx, "the picture while meta is out", |cx| {
        cx.debug_bounds("pv1-picture")
    });
    for (side, own) in [(rect.size.width, 320.), (rect.size.height, 240.)] {
        assert!(
            (f32::from(side) - own).abs() <= 1.,
            "the picture draws at its own {own} pixels before meta lands: {rect:?}"
        );
    }
    // Only the header read knows the shape while meta is out, so the size came from it and not from the decode.
    let known = look(&shell, cx).read_with(cx, |look, _| {
        look.subject().is_some_and(|subject| subject.shape.known)
    });
    assert!(known, "the header read landed the picture's shape");
    close(&shell, cx);
}
