//! The two marks a pane state draws: the brand spiral drawing itself in, and a 40 px state glyph from the monochrome atlas.
use super::brand::{self, GRID};
use gpui::{
    fill, point, px, size, App, Bounds, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, SharedString, Style, TransformationMatrix,
    Window,
};

/// The hero mark's side, three whole 22 px Omi grids.
pub const HERO: f32 = 66.;
/// A failure or no-match mark's side, board States at 40.
pub const STATE: f32 = 40.;
/// The spiral's centreline length in grid units.
pub const LENGTH: f32 = 114.;

/// The spiral with `drawn` of its length painted, at `opacity`, as quads like the loading mark.
pub struct Spiral {
    pub side: f32,
    pub drawn: f32,
    pub opacity: f32,
    pub ink: Hsla,
}

/// The glyphs a pane state stands on: the Omarchy cut paths ui/js/Icons.js carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Lock,
    Alert,
    Search,
}

macro_rules! svg {
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

impl Glyph {
    pub fn svg(self) -> (SharedString, &'static [u8]) {
        let (key, data) = match self {
            Self::Lock => (
                "flea-state:lock",
                svg!("M4 11h16v10H4z M8 11V7a4 4 0 0 1 8 0v4 M12 15h.01"),
            ),
            Self::Alert => (
                "flea-state:alert",
                svg!("M12 2 23 21H1z M12 10v4 M12 18L12.01 18"),
            ),
            Self::Search => (
                "flea-state:search",
                svg!("M16.7 16.7 21 21 M3 11A8 8 0 1 0 19 11A8 8 0 1 0 3 11Z"),
            ),
        };
        (SharedString::new_static(key), data)
    }
}

/// A glyph in its state slot.
pub struct StateMark {
    pub side: f32,
    pub glyph: Glyph,
    pub ink: Hsla,
}

fn square(side: f32, window: &mut Window, cx: &mut App) -> LayoutId {
    let style = Style {
        size: size(px(side).into(), px(side).into()),
        flex_shrink: 0.,
        ..Style::default()
    };
    window.request_layout(style, None, cx)
}

macro_rules! leaf {
    ($ty:ty, $side:ident, |$this:ident, $bounds:ident, $window:ident, $cx:ident| $paint:block) => {
        impl IntoElement for $ty {
            type Element = Self;
            fn into_element(self) -> Self {
                self
            }
        }

        impl Element for $ty {
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
                (square(self.$side, window, cx), ())
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
                $bounds: Bounds<Pixels>,
                _: &mut (),
                _: &mut (),
                $window: &mut Window,
                $cx: &mut App,
            ) {
                let $this = &*self;
                $paint
            }
        }
    };
}

leaf!(Spiral, side, |this, bounds, window, _cx| {
    if this.drawn <= 0. || this.opacity <= 0. {
        return;
    }
    let scale = window.scale_factor();
    let side = bounds.size.width.min(bounds.size.height);
    let unit = f32::from(side) * scale / GRID;
    let left = (f32::from(bounds.origin.x + (bounds.size.width - side) / 2.) * scale).round();
    let top = (f32::from(bounds.origin.y + (bounds.size.height - side) / 2.) * scale).round();
    let (ink, opacity) = (this.ink, this.opacity);
    brand::drawn(LENGTH * this.drawn, unit, |[x0, y0, x1, y1], coverage| {
        let quad = Bounds::from_corners(
            point(px((left + x0) / scale), px((top + y0) / scale)),
            point(px((left + x1) / scale), px((top + y1) / scale)),
        );
        window.paint_quad(fill(quad, ink.opacity(coverage * opacity)));
    });
});

leaf!(StateMark, side, |this, bounds, window, cx| {
    let (key, data) = this.glyph.svg();
    let _ = window.paint_svg(
        bounds,
        key,
        Some(data),
        TransformationMatrix::unit(),
        this.ink,
        cx,
    );
});
