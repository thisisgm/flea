//! One board glyph as an alpha mask in its own box, for the strip's chrome buttons.
use super::board::Glyph;
use gpui::{
    px, size, App, Bounds, Element, ElementId, GlobalElementId, Hsla, InspectorElementId,
    IntoElement, LayoutId, Pixels, Style, TransformationMatrix, Window,
};

pub struct GlyphBox {
    pub glyph: Glyph,
    pub side: f32,
    pub ink: Hsla,
}

impl IntoElement for GlyphBox {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for GlyphBox {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: size(px(self.side).into(), px(self.side).into()),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let (key, data) = match self.glyph {
            // Back points to logical leading, right in a mirrored window.
            Glyph::Back if window.layout_mirrored() => (
                crate::locale_geometry::ARROW_RIGHT.0.into(),
                crate::locale_geometry::ARROW_RIGHT.1,
            ),
            glyph => glyph.svg(),
        };
        let _ = window.paint_svg(
            bounds,
            key,
            Some(data),
            TransformationMatrix::unit(),
            self.ink,
            cx,
        );
    }
}
