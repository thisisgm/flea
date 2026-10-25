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
    // Besides the rail and the full-width status bar, only the open card paints the surface.
    assert_ne!(theme.surface, theme.background);
    let viewport = window.viewport_size();
    let full_width = |quad: &Quad| {
        (quad.bounds.size.width.as_f32() - f32::from(viewport.width) * scale).abs() <= 2.
    };
    assert!(
        quads.iter().any(|quad| !rail_sized(quad)
            && !full_width(quad)
            && quad.background.as_solid() == Some(theme.surface)),
        "the actual open card surface did not repaint"
    );
    // HIG list: the status bar stands on --surface along the window's bottom edge.
    let bottom = |quad: &Quad| {
        let edge = quad.bounds.origin.y + quad.bounds.size.height;
        (edge.as_f32() - f32::from(viewport.height) * scale).abs() <= 2.
    };
    assert!(
        quads.iter().any(|quad| full_width(quad)
            && bottom(quad)
            && quad.background.as_solid() == Some(theme.surface)),
        "the status bar surface did not repaint"
    );
    // HIG list: the chrome strip stands on the 55% surface mix along the window's top edge.
    assert!(
        quads.iter().any(|quad| full_width(quad)
            && quad.bounds.origin.y.as_f32().abs() <= 2.
            && quad.background.as_solid() == Some(theme.chrome())),
        "the chrome strip ground did not repaint"
    );
    ground(
        &quads,
        Bounds::new(
            gpui::point(gpui::px(0.), gpui::px(0.)),
            window.viewport_size(),
        ),
        theme.background,
        "window canvas",
        scale,
    );
}
