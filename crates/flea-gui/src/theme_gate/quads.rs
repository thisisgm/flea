//! Assert the actual rendered scene's grounds, alongside the per-frame ink receipts.
use crate::{palette::Theme, state::Shell};
use gpui::{App, Bounds, Hsla, Pixels, Quad, Window};

fn ground(quads: &[Quad], bounds: Bounds<Pixels>, ink: Hsla, name: &str, scale: f32) {
    let want = [
        bounds.origin.x,
        bounds.origin.y,
        bounds.size.width,
        bounds.size.height,
    ]
    .map(|v| f32::from(v) * scale);
    assert!(
        quads.iter().any(|quad| {
            let got = [
                quad.bounds.origin.x,
                quad.bounds.origin.y,
                quad.bounds.size.width,
                quad.bounds.size.height,
            ]
            .map(|v| v.as_f32());
            got.iter()
                .zip(want)
                .all(|(got, want)| (*got - want).abs() <= 2.)
                && quad.background.as_solid() == Some(ink)
        }),
        "{name}: no actual scene quad paints {ink:?} at {want:?}"
    );
}

pub(super) fn assert(shell: &Shell, window: &Window, cx: &App) {
    if cx.try_global::<super::frame::Frames>().is_none() {
        return;
    }
    let theme = *cx.global::<Theme>();
    let quads = window.painted_quads();
    let scale = window.scale_factor();
    for (side, host) in shell.panes.iter().enumerate() {
        let host = host.as_ref().expect("both real panes");
        ground(
            &quads,
            host.list.borrow().drawn.bounds,
            theme.background,
            if side == 0 { "left pane" } else { "right pane" },
            scale,
        );
    }
    let rail = shell.rail.as_ref().unwrap().places.read(cx).laid.borrow();
    let rail_sized = |quad: &Quad| {
        (quad.bounds.size.width.as_f32() - rail.width * scale).abs() <= 2.
            && (quad.bounds.size.height.as_f32() - rail.height * scale).abs() <= 2.
    };
    // The rail stands on the surface, as Sidebar.qml and the HIG boards draw it.
    assert!(
        quads
            .iter()
            .any(|quad| rail_sized(quad) && quad.background.as_solid() == Some(theme.surface)),
        "the actual rail ground did not repaint"
    );
    // These fixtures have distinct canvas/card grounds. Only the open card paints the opaque surface.
    // Debug selectors are compiled out of release GPUI, so this reads the scene primitive directly.
    assert_ne!(theme.surface, theme.background);
    assert!(
        quads
            .iter()
            .any(|quad| !rail_sized(quad) && quad.background.as_solid() == Some(theme.surface)),
        "the actual open card surface did not repaint"
    );
    // Footer has no fill of its own: its ground is the window canvas, dimmed by the same modal scrim.
    ground(
        &quads,
        Bounds::new(
            gpui::point(gpui::px(0.), gpui::px(0.)),
            window.viewport_size(),
        ),
        theme.background,
        "window/footer canvas",
        scale,
    );
}
