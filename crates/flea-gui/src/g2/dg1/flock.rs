//! Carry's flock, painted beside the resting pointer: each lifted mark on its own ground plate, back first, and the front row's name after it, as board carry.html draws it.
use super::carry::{Flock, GLYPH, LAYERS, NAME_AT, NAME_SIZE, OFFSET, OPACITY};
use crate::row::marks::Mark;
use gpui::{
    point, px, size, App, Bounds, Element, ElementId, Font, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, ShapedLine, SharedString, Style,
    TextRun, TransformationMatrix, Window,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
pub(super) struct NameCache {
    font: Font,
    ink: Hsla,
    scale: f32,
    text: SharedString,
    line: Option<ShapedLine>,
}

/// The translucent flock beside the pointer, board carry.html: each mark on its own ground plate, the front row's name after it.
#[derive(Clone)]
pub struct FlockElement {
    pub flock: Rc<Flock>,
    pub(super) name: Rc<RefCell<NameCache>>,
    pub at: Point<Pixels>,
    pub ground: Hsla,
    pub ink: Hsla,
}

impl FlockElement {
    pub fn new(flock: Rc<Flock>, at: Point<Pixels>, ground: Hsla, ink: Hsla) -> Self {
        Self {
            flock,
            name: Rc::default(),
            at,
            ground,
            ink,
        }
    }
    /// Each painted layer, back first: its mark, its glyph origin and its opacity under the flock's own.
    pub fn layers(
        &self,
    ) -> impl DoubleEndedIterator<Item = (Mark, Point<Pixels>, f32)> + ExactSizeIterator + '_ {
        let origin = self.origin();
        self.flock
            .marks
            .iter()
            .zip(LAYERS)
            .rev()
            .map(move |(mark, (dx, dy, alpha))| {
                (
                    *mark,
                    point(origin.x + px(dx), origin.y + px(dy)),
                    alpha * OPACITY,
                )
            })
    }

    pub fn origin(&self) -> Point<Pixels> {
        point(self.at.x + px(OFFSET.0), self.at.y + px(OFFSET.1))
    }
}

impl IntoElement for FlockElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for FlockElement {
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
            position: gpui::Position::Absolute,
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
        _: &mut App,
    ) {
        let font = window.text_style().font();
        let scale = window.scale_factor();
        let mut cache = self.name.borrow_mut();
        if cache.font == font
            && cache.ink == self.ink
            && cache.scale == scale
            && cache.text.as_ref() == self.flock.name.as_str()
        {
            return;
        }
        cache.font = font.clone();
        cache.ink = self.ink;
        cache.scale = scale;
        cache.text = self.flock.name.clone().into();
        let run = TextRun {
            len: cache.text.len(),
            font,
            color: self.ink.opacity(OPACITY),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        cache.line = (!cache.text.is_empty()).then(|| {
            window
                .text_system()
                .shape_line(cache.text.clone(), px(NAME_SIZE), &[run], None)
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        // The pointer and every layer are window coordinates already, so paint needs no offset.
        for (mark, at, alpha) in self.layers() {
            let plate = Bounds::new(at, size(px(GLYPH), px(GLYPH)));
            window.paint_quad(gpui::fill(plate, self.ground.opacity(alpha)));
            let (key, data) = mark.svg();
            let _ = window.paint_svg(
                plate,
                key,
                Some(data),
                TransformationMatrix::unit(),
                self.ink.opacity(alpha),
                cx,
            );
        }
        let cache = self.name.borrow();
        let Some(line) = cache.line.as_ref() else {
            return;
        };
        let origin = self.origin();
        let _ = line.paint(
            point(origin.x + px(NAME_AT.0), origin.y + px(NAME_AT.1)),
            px(GLYPH),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}
