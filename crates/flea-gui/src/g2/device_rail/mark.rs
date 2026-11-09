//! The unlock card's cut marks: a Lucide path at a fixed size, painted with no allocation in paint.
use gpui::{
    prelude::*, px, App, Bounds, Element, ElementId, GlobalElementId, Hsla, InspectorElementId,
    LayoutId, Pixels, SharedString, Style, TransformationMatrix, Window,
};

/// A Lucide path drawn in the cut: square caps, mitred joins.
macro_rules! mark_svg {
    ($d:literal) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" "#,
            r#"stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d=""#,
            $d,
            r#""/></svg>"#
        )
        .as_bytes()
    };
}

pub(super) const DRIVE: (&str, &[u8]) = (
    "flea-mark:drive",
    mark_svg!("M6 4h12l4 8v8H2v-8z M2 12h20 M6 16L6.01 16 M10 16L10.01 16"),
);
pub(super) const EYE: (&str, &[u8]) = (
    "flea-mark:eye",
    mark_svg!("M12 5 22 12 12 19 2 12z M10 10h4v4h-4z"),
);

/// One cut mark at a fixed size, painted from its path with no allocation in paint.
pub struct Mark {
    key: &'static str,
    data: &'static [u8],
    size: f32,
    ink: Hsla,
}

impl Mark {
    pub fn new(mark: (&'static str, &'static [u8]), size: f32, ink: Hsla) -> Self {
        Self {
            key: mark.0,
            data: mark.1,
            size,
            ink,
        }
    }
}

impl IntoElement for Mark {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Mark {
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
        let mut style = Style::default();
        style.size.width = px(self.size).into();
        style.size.height = px(self.size).into();
        style.flex_shrink = 0.;
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
        let key = SharedString::new_static(self.key);
        let _ = window.paint_svg(
            bounds,
            key,
            Some(self.data),
            TransformationMatrix::unit(),
            self.ink,
            cx,
        );
    }
}
