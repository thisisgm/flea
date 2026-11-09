//! The shared field painter reads each host through its existing ownership checks.
use super::host::Overlay;
use gpui::{App, Window};

pub(crate) trait FieldView: gpui::EntityInputHandler + Sized {
    fn field_entry(&self) -> &crate::path::entry::Entry;
    fn field_drawn(&self) -> &std::cell::RefCell<super::editor::Drawn>;
    fn field_active(&self, window: &Window, cx: &App) -> bool;
    fn field_palette(&self) -> super::palette::CardPalette;
    fn field_focus(&self) -> Option<gpui::FocusHandle>;
    fn field_ink(&self) -> gpui::Hsla {
        self.field_palette().foreground
    }
    fn field_held(&self) -> usize {
        0
    }
    fn field_revealed(&self) -> bool {
        false
    }
    fn field_press(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    );
    fn field_drag(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    );
}

impl FieldView for Overlay {
    fn field_entry(&self) -> &crate::path::entry::Entry {
        &self.editor.entry
    }
    fn field_drawn(&self) -> &std::cell::RefCell<super::editor::Drawn> {
        &self.editor.drawn
    }
    fn field_active(&self, window: &Window, cx: &App) -> bool {
        self.editing_copy(window, cx)
    }
    fn field_palette(&self) -> super::palette::CardPalette {
        self.palette
    }
    fn field_focus(&self) -> Option<gpui::FocusHandle> {
        self.editor.focus.clone()
    }
    fn field_press(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.copy_press(event, window, cx);
    }
    fn field_drag(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.copy_drag(event, window, cx);
    }
}

pub(crate) fn display<V: FieldView>(view: &V) -> std::borrow::Cow<'_, str> {
    let entry = view.field_entry();
    if entry.text.is_empty() && view.field_held() > 0 {
        "\u{2022}".repeat(view.field_held()).into()
    } else if view.field_revealed() {
        std::borrow::Cow::Borrowed(&entry.text)
    } else {
        entry.display()
    }
}

pub(crate) fn caret<V: FieldView>(
    view: &V,
    line: &crate::path::field::Line,
    byte: usize,
) -> gpui::Pixels {
    let entry = view.field_entry();
    if entry.text.is_empty() && view.field_held() > 0 {
        line.run
            .as_ref()
            .map_or(gpui::px(0.), |run| run.line().width)
    } else if view.field_revealed() {
        line.caret_x(byte)
    } else {
        line.entry_caret(entry, byte)
    }
}

pub(crate) fn span<V: FieldView>(
    view: &V,
    line: &crate::path::field::Line,
    range: &std::ops::Range<usize>,
) -> (gpui::Pixels, gpui::Pixels) {
    if range.is_empty() {
        let at = caret(view, line, range.start);
        (at, at)
    } else if view.field_revealed() {
        line.span(range)
    } else {
        line.entry_span(view.field_entry(), range)
    }
}

pub(crate) fn bounds<V: FieldView>(
    view: &V,
    range: &std::ops::Range<usize>,
    field: gpui::Bounds<gpui::Pixels>,
) -> gpui::Bounds<gpui::Pixels> {
    let drawn = view.field_drawn().borrow();
    let (from, to) = span(view, &drawn.line, &view.field_entry().ime_range(range));
    let left = field.left() - drawn.line.scroll;
    gpui::Bounds::from_corners(
        gpui::point(left + from, field.top()),
        gpui::point(left + to, field.bottom()),
    )
}

pub(crate) fn index<V: FieldView>(view: &V, x: gpui::Pixels) -> usize {
    let drawn = view.field_drawn().borrow();
    let byte = drawn.line.index_for(x + drawn.line.scroll);
    if view.field_revealed() {
        byte.min(view.field_entry().text.len())
    } else {
        view.field_entry().from_display(byte)
    }
}
