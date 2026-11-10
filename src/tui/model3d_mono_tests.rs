use super::tests::{model, prime, settle, step, Fixture};
use crate::tui::{
    graphics::{palette, Graphics, Protocol},
    stage,
    theme::{Depth, Theme},
    wire,
};

#[test]
fn mono_model_raster_uses_graphics_without_an_ansi_colour_mode() {
    let root = Fixture::new();
    for protocol in [Protocol::Kitty, Protocol::Sixel] {
        let mut m = model(&root);
        m.preview_state.colors = None;
        m.model3d_palette = palette(&Theme::with_depth("", Depth::Mono));
        let (mut wire, reader) = wire::echo_wire();
        let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
        graphics.protocol = protocol;
        m.capabilities.graphics = protocol;
        prime(&mut m, &root, &mut graphics, &mut shown, &mut wire);
        step(&mut m, &mut graphics, &mut shown, &mut wire, false);
        assert!(
            m.model3d.rendering.is_some(),
            "{protocol:?}: Mono suppressed raster rendering"
        );
        settle(|| {
            step(&mut m, &mut graphics, &mut shown, &mut wire, false);
            !graphics.bytes.is_empty() || m.model3d.failed
        });
        assert!(!m.model3d.failed, "{protocol:?}: raster transport failed");
        let raster = &m.model3d.ready.as_ref().unwrap().rgba;
        let ground = flea::ui_logic_model_preview::color::to_bytes(m.model3d_palette.ground);
        assert!(raster.chunks_exact(4).any(|p| p[..3] != ground));
        assert!(!raster.is_empty());
        wire::finish(wire, reader);
    }
}

#[test]
fn mono_without_a_graphics_protocol_keeps_the_metadata_fallback() {
    let root = Fixture::new();
    let mut m = model(&root);
    m.preview_state.colors = None;
    m.capabilities.graphics = Protocol::None;
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    prime(&mut m, &root, &mut graphics, &mut shown, &mut wire);
    step(&mut m, &mut graphics, &mut shown, &mut wire, false);
    assert!(
        m.model3d.data.is_some(),
        "loaded metadata must survive Mono"
    );
    assert!(m.model3d.rendering.is_none());
    assert!(graphics.bytes.is_empty());
    wire::finish(wire, reader);
}
