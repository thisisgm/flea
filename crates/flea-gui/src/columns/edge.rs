//! The preview edge's hover paint: the accent line on the hairline and the move-horizontal mark beside it, over the area.
use super::glyphs::{paint_svg, MOVE_SVG};
use super::paint::Palette;
use crate::palette::Theme;
use gpui::{
    fill, point, px, relative, size, App, Bounds, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window,
};

/// What the edge draws this frame in the area's own pixels, read once per event.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgePaint {
    pub line: Option<(f32, f32, f32, f32)>,
    pub mark: Option<(f32, f32, f32)>,
}

impl super::Area {
    pub fn edge_paint(&self) -> EdgePaint {
        let height = self.tracks.height;
        EdgePaint {
            line: self.host.edge.line(&self.layout, height),
            mark: self.host.edge.mark(&self.layout, height),
        }
    }
}

/// One overlay Element over the whole area; at rest it paints nothing.
pub struct EdgeElement {
    paint: EdgePaint,
}

impl EdgeElement {
    pub fn new(paint: EdgePaint) -> Self {
        Self { paint }
    }
}

impl IntoElement for EdgeElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for EdgeElement {
    type RequestLayoutState = ();
    type PrepaintState = Palette;

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
            size: size(relative(1.).into(), relative(1.).into()),
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
        window: &mut Window,
        cx: &mut App,
    ) -> Palette {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        Palette::inherited(&window.text_style(), theme)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        palette: &mut Palette,
        window: &mut Window,
        cx: &mut App,
    ) {
        let origin = bounds.origin;
        if let Some((x, y, width, height)) = self.paint.line {
            let at = point(origin.x + px(x), origin.y + px(y));
            window.paint_quad(fill(
                Bounds::new(at, size(px(width), px(height))),
                palette.accent,
            ));
        }
        if let Some((x, y, side)) = self.paint.mark {
            let at = point(origin.x + px(x), origin.y + px(y));
            paint_svg(
                MOVE_SVG,
                Bounds::new(at, size(px(side), px(side))),
                palette.foreground,
                window,
                cx,
            );
        }
    }
}
