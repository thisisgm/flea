//! Bounded visible text lines, shaped outside paint and retained for the completed frame.
use crate::list::Typeface;
use gpui::{
    point, px, relative, App, Bounds, Element, ElementId, GlobalElementId, GlyphRun, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, TextRun, Window,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
pub(crate) struct Cache {
    key: Option<(Typeface, u32, Hsla)>,
    lines: Vec<(String, GlyphRun)>,
}
pub(crate) struct Text {
    lines: Rc<[String]>,
    cache: Rc<RefCell<Cache>>,
    face: Typeface,
    ink: Hsla,
}
impl Text {
    pub(crate) fn new(
        lines: Rc<[String]>,
        cache: Rc<RefCell<Cache>>,
        face: Typeface,
        ink: Hsla,
    ) -> Self {
        Self {
            lines,
            cache,
            face,
            ink,
        }
    }
}
impl IntoElement for Text {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Text {
    type RequestLayoutState = ();
    type PrepaintState = (usize, f32);
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
        (
            window.request_layout(
                Style {
                    size: gpui::size(relative(1.).into(), relative(1.).into()),
                    ..Style::default()
                },
                None,
                cx,
            ),
            (),
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
        let size = f32::from(self.face.caption);
        let height = (size * 1.4).round();
        let count =
            ((f32::from(bounds.size.height) / height).ceil() as usize).min(self.lines.len());
        let cells = (f32::from(bounds.size.width) / (size * 0.6))
            .floor()
            .max(0.) as usize;
        let mut cache = self.cache.borrow_mut();
        let key = (self.face.clone(), cells as u32, self.ink);
        if cache.key.as_ref() != Some(&key) {
            cache.key = Some(key);
            cache.lines.clear();
        }
        for at in cache.lines.len()..count {
            let text: String = self.lines[at].chars().take(cells).collect();
            let run = TextRun {
                len: text.len(),
                font: self.face.font.clone(),
                color: self.ink,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = GlyphRun::new(window.text_system().layout_line(
                &text,
                px(size),
                &[run],
                None,
            ));
            cache.lines.push((text, shaped));
        }
        cache.lines.truncate(count);
        (count, height)
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        (count, height): &mut Self::PrepaintState,
        window: &mut Window,
        _: &mut App,
    ) {
        let cache = self.cache.borrow();
        for (at, (_, run)) in cache.lines.iter().take(*count).enumerate() {
            let line = run.line();
            let y = bounds.origin.y + px(at as f32 * *height) + line.ascent;
            let _ = window.paint_glyph_run(point(bounds.origin.x, y), run, self.ink);
        }
    }
}
