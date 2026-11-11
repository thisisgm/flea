//! Card labels keep the authored line height through measurement and baseline paint.
use gpui::{
    px, size, App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Length, Pixels, ShapedLine, SharedString, Style, TextAlign,
    TextOverflow, TruncateFrom, Window,
};
use std::{cell::RefCell, rc::Rc};

pub(super) struct Line(SharedString);

impl Line {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self(text.into())
    }
}

pub(super) struct Layout {
    shaped: Rc<RefCell<ShapedLine>>,
    height: Pixels,
    align: TextAlign,
}

impl IntoElement for Line {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Line {
    type RequestLayoutState = Layout;
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
        _: &mut App,
    ) -> (LayoutId, Layout) {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        // Measured leaves round up in GPUI. Keep the fractional CSS line here and at paint.
        let height = style
            .line_height
            .to_pixels(font_size.into(), window.rem_size());
        let text = self.0.clone();
        let run = style.to_run(text.len());
        let original = window.text_system().shape_line(
            text.clone(),
            font_size,
            std::slice::from_ref(&run),
            None,
        );
        let shaped = Rc::new(RefCell::new(original.clone()));
        let measured = shaped.clone();
        let align = style.text_align;
        // An ellipsizing flex leaf must be allowed below its untruncated min-content width.
        let mut leaf = Style::default();
        if style.text_overflow.is_some() {
            leaf.min_size.width = Length::from(px(0.));
        }
        let id = window.request_measured_layout(leaf, move |known, available, window, cx| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(width) => Some(width),
                _ => None,
            });
            let line = if let (Some(overflow), Some(width)) = (&style.text_overflow, width) {
                if original.width() > width {
                    let (affix, from) = match overflow {
                        TextOverflow::Truncate(affix) => (affix, TruncateFrom::End),
                        TextOverflow::TruncateStart(affix) => (affix, TruncateFrom::Start),
                        TextOverflow::TruncateMiddle(affix) => (affix, TruncateFrom::Middle),
                    };
                    let (text, runs) = cx
                        .text_system()
                        .line_wrapper(style.font(), font_size)
                        .truncate_line(
                            text.clone(),
                            width,
                            affix,
                            std::slice::from_ref(&run),
                            from,
                        );
                    window
                        .text_system()
                        .shape_line(text, font_size, &runs, None)
                } else {
                    original.clone()
                }
            } else {
                original.clone()
            };
            let size = size(line.width().ceil(), height);
            *measured.borrow_mut() = line;
            size
        });
        (
            id,
            Layout {
                shaped,
                height,
                align,
            },
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Layout,
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Layout,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let line = layout.shaped.borrow();
        #[cfg(test)]
        super::fix10_tests::record(&self.0, bounds, layout.height, &line, window, cx);
        line.paint(
            bounds.origin,
            layout.height,
            layout.align,
            Some(bounds.size.width),
            window,
            cx,
        )
        .expect("card label paints");
    }
}
