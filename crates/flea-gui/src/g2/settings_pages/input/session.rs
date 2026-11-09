//! Platform callbacks belong to the editing session and focus owner that registered them.
use super::Host;
use gpui::{
    App, Bounds, ClipboardItem, Context, Entity, EntityInputHandler, FocusHandle, InputHandler,
    Pixels, Point, TextInputConfiguration, UTF16Selection, WeakEntity, Window,
};
use std::ops::Range;

pub(super) struct SessionInput<T> {
    view: WeakEntity<T>,
    generation: u64,
    focus: FocusHandle,
    bounds: Bounds<Pixels>,
}

impl<T: Host + EntityInputHandler> SessionInput<T> {
    pub fn new(
        bounds: Bounds<Pixels>,
        view: &Entity<T>,
        generation: u64,
        focus: FocusHandle,
    ) -> Self {
        Self {
            view: view.downgrade(),
            generation,
            focus,
            bounds,
        }
    }

    fn with<R>(
        &self,
        window: &mut Window,
        cx: &mut App,
        callback: impl FnOnce(&mut T, &mut Window, &mut Context<T>) -> R,
    ) -> Option<R> {
        self.view
            .update(cx, |view, cx| {
                (view.editor().entry.session() == Some(self.generation)
                    && view.input_focus(window) == &self.focus
                    && view.accepts_input(window))
                .then(|| callback(view, window, cx))
            })
            .ok()
            .flatten()
    }
}

impl<T: Host + EntityInputHandler> InputHandler for SessionInput<T> {
    fn set_ime_cursor_visible(&mut self, visible: bool, window: &mut Window, cx: &mut App) {
        self.with(window, cx, |view, window, cx| {
            view.set_ime_cursor_visible(visible, window, cx)
        });
    }

    fn selected_text_range(
        &mut self,
        ignore_disabled: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.with(window, cx, |view, window, cx| {
            view.selected_text_range(ignore_disabled, window, cx)
        })
        .flatten()
    }

    fn marked_text_range(&mut self, window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.with(window, cx, |view, window, cx| {
            view.marked_text_range(window, cx)
        })
        .flatten()
    }

    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.with(window, cx, |view, window, cx| {
            view.text_for_range(range, adjusted, window, cx)
        })
        .flatten()
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.with(window, cx, |view, window, cx| {
            view.replace_text_in_range(range, text, window, cx)
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.with(window, cx, |view, window, cx| {
            view.replace_and_mark_text_in_range(range, text, selected, window, cx)
        });
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut App) {
        self.with(window, cx, |view, window, cx| view.unmark_text(window, cx));
    }

    fn paste(&mut self, item: ClipboardItem, window: &mut Window, cx: &mut App) {
        self.with(window, cx, |view, window, cx| view.paste(item, window, cx));
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.with(window, cx, |view, window, cx| {
            view.bounds_for_range(range, self.bounds, window, cx)
        })
        .flatten()
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        self.with(window, cx, |view, window, cx| {
            view.character_index_for_point(point, window, cx)
        })
        .flatten()
    }

    fn set_selected_text_range(
        &mut self,
        range: UTF16Selection,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.with(window, cx, |view, window, cx| {
            view.set_selected_text_range(range, window, cx)
        });
    }

    fn element_bounds(&mut self, window: &mut Window, cx: &mut App) -> Option<Bounds<Pixels>> {
        self.with(window, cx, |_, _, _| self.bounds)
    }

    fn text_length_utf16(&mut self, window: &mut Window, cx: &mut App) -> Option<usize> {
        self.with(window, cx, |view, window, cx| {
            view.text_length_utf16(window, cx)
        })
        .flatten()
    }

    fn accepts_text_input(&mut self, window: &mut Window, cx: &mut App) -> bool {
        self.with(window, cx, |view, window, cx| {
            view.accepts_text_input(window, cx)
        })
        .unwrap_or(false)
    }

    fn text_input_editable_range(
        &mut self,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Range<usize>> {
        self.with(window, cx, |view, window, cx| {
            view.text_input_editable_range(window, cx)
        })
        .flatten()
    }

    fn prefers_ime_for_printable_keys(&mut self, window: &mut Window, cx: &mut App) -> bool {
        self.accepts_text_input(window, cx)
    }

    fn text_input_configuration(
        &mut self,
        window: &mut Window,
        cx: &mut App,
    ) -> TextInputConfiguration {
        self.with(window, cx, |view, window, cx| {
            view.text_input_configuration(window, cx)
        })
        .unwrap_or_default()
    }
}
