//! TabBar.qml under glass: the strip's surface runs left and right of the current tab's plate, never under it.
use super::*;

#[gpui::test]
fn pb_tabs_glass_strip_ground_is_split_around_the_current_plate(cx: &mut TestAppContext) {
    let fixture = install(
        cx,
        "pb-tabs-glass-strip",
        &[(2, "[flea]\nbackground-alpha=0.34")],
    );
    let handle = launch(cx);
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                assert!(shell.tab_action("tabNew", window, cx));
            })
        })
        .unwrap();
    cx.run_until_parked();
    draw(handle, cx);
    draw(handle, cx);
    handle
        .update(cx, |root, window, cx| {
            let shell = root.shell.read(cx);
            let strip = shell.strip.bounds.get().expect("two tabs mount the strip");
            let frame = &shell.strip.snapshot.as_ref().unwrap().frame;
            let left = f32::from(strip.left())
                + crate::tabs::bar::PAD_X
                + frame.slots[frame.current] as f32 * frame.tab_width;
            let scale = window.scale_factor();
            let quads = window.painted_quads();
            // The glass ground is translucent; the plate paints the opaque background.
            let ground = |x: f32, width: f32| {
                quads.iter().any(|quad| {
                    (quad.bounds.origin.x.as_f32() - x * scale).abs() < 0.5
                        && (quad.bounds.origin.y.as_f32() - f32::from(strip.top()) * scale).abs()
                            < 0.5
                        && (quad.bounds.size.width.as_f32() - width * scale).abs() < 0.5
                        && quad
                            .background
                            .as_solid()
                            .is_some_and(|color| color.a > 0. && color.a < 1.)
                })
            };
            let right = left + frame.tab_width;
            assert!(ground(
                f32::from(strip.left()),
                left - f32::from(strip.left())
            ));
            assert!(ground(right, f32::from(strip.right()) - right));
            assert!(
                !ground(f32::from(strip.left()), f32::from(strip.size.width)),
                "the plate never sits on a whole-strip glass ground"
            );
            root.shell.update(cx, |shell, _| shell.stop_for_app_quit());
        })
        .unwrap();
    fixture.cleanup();
}

// EY-430: an opaque strip is theme.surface, so the current tab's background plate stands out of it.
#[gpui::test]
fn ey430_opaque_strip_paints_surface_under_a_background_plate(cx: &mut TestAppContext) {
    let fixture = install(
        cx,
        "ey430-opaque-strip",
        &[(
            0,
            "background='#101315'\nforeground='#cacccc'\ndark_background='#08090a'",
        )],
    );
    let handle = launch(cx);
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                assert!(shell.tab_action("tabNew", window, cx));
            })
        })
        .unwrap();
    cx.run_until_parked();
    draw(handle, cx);
    draw(handle, cx);
    handle
        .update(cx, |root, window, cx| {
            assert!(
                crate::window::glass::alpha(window, cx) >= 1.,
                "the control: no glass"
            );
            let theme = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default();
            assert_ne!(
                gpui::Rgba::from(theme.surface),
                gpui::Rgba::from(theme.background),
                "the fixture theme must tell surface from background"
            );
            let shell = root.shell.read(cx);
            let strip = shell.strip.bounds.get().expect("two tabs mount the strip");
            let frame = &shell.strip.snapshot.as_ref().unwrap().frame;
            let plate_x = f32::from(strip.left())
                + crate::tabs::bar::PAD_X
                + frame.slots[frame.current] as f32 * frame.tab_width;
            let scale = window.scale_factor();
            let quads = window.painted_quads();
            // The scene keeps quads in draw order; the last match is the one on top.
            let painted = |x: f32, width: f32, color: gpui::Hsla| {
                quads.iter().rposition(|quad| {
                    (quad.bounds.origin.x.as_f32() - x * scale).abs() < 0.5
                        && (quad.bounds.origin.y.as_f32() - f32::from(strip.top()) * scale).abs()
                            < 0.5
                        && (quad.bounds.size.width.as_f32() - width * scale).abs() < 0.5
                        && quad
                            .background
                            .as_solid()
                            .is_some_and(|got| gpui::Rgba::from(got) == gpui::Rgba::from(color))
                })
            };
            let surface = painted(
                f32::from(strip.left()),
                f32::from(strip.size.width),
                theme.surface,
            )
            .expect("the whole strip is theme.surface");
            assert_eq!(
                painted(
                    f32::from(strip.left()),
                    f32::from(strip.size.width),
                    theme.background
                ),
                None,
                "the strip is never the window background"
            );
            let plate = painted(plate_x, frame.tab_width, theme.background)
                .expect("the current tab paints its own background plate");
            assert!(plate > surface, "the plate paints over the strip surface");
            root.shell.update(cx, |shell, _| shell.stop_for_app_quit());
        })
        .unwrap();
    fixture.cleanup();
}
