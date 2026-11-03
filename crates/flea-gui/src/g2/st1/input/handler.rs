//! Each real host registers its own platform handler, with shared session and focus guards.
macro_rules! input_handler {
    ($host:ty) => {
        impl gpui::EntityInputHandler for $host {
            fn accepts_text_input(
                &self,
                window: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> bool {
                use $crate::g2::st1::input::Host;
                self.accepts_input(window)
            }

            fn text_for_range(
                &mut self,
                range: std::ops::Range<usize>,
                adjusted: &mut Option<std::ops::Range<usize>>,
                window: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> Option<String> {
                use $crate::g2::st1::input::Host;
                self.accepts_input(window)
                    .then(|| self.editor().entry.ime_text(&range, adjusted))
                    .flatten()
            }

            fn selected_text_range(
                &mut self,
                _: bool,
                window: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> Option<gpui::UTF16Selection> {
                use $crate::g2::st1::input::Host;
                self.accepts_input(window)
                    .then(|| self.editor().entry.ime_selection())
            }

            fn marked_text_range(
                &self,
                window: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> Option<std::ops::Range<usize>> {
                use $crate::g2::st1::input::Host;
                self.accepts_input(window)
                    .then(|| self.editor().entry.ime_marked())
                    .flatten()
            }

            fn unmark_text(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) {
                use $crate::g2::st1::input::Host;
                if self.accepts_input(window) && self.editor_mut().entry.ime_unmark() {
                    self.edited(cx);
                }
            }

            fn replace_text_in_range(
                &mut self,
                range: Option<std::ops::Range<usize>>,
                text: &str,
                window: &mut gpui::Window,
                cx: &mut gpui::Context<Self>,
            ) {
                use $crate::g2::st1::input::Host;
                if self.accepts_input(window) && self.editor_mut().entry.ime_replace(range, text) {
                    self.edited(cx);
                }
            }

            fn replace_and_mark_text_in_range(
                &mut self,
                range: Option<std::ops::Range<usize>>,
                text: &str,
                selected: Option<std::ops::Range<usize>>,
                window: &mut gpui::Window,
                cx: &mut gpui::Context<Self>,
            ) {
                use $crate::g2::st1::input::Host;
                if self.accepts_input(window)
                    && self.editor_mut().entry.ime_mark(range, text, selected)
                {
                    self.edited(cx);
                }
            }

            fn bounds_for_range(
                &mut self,
                range: std::ops::Range<usize>,
                _: gpui::Bounds<gpui::Pixels>,
                window: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> Option<gpui::Bounds<gpui::Pixels>> {
                use $crate::g2::st1::input::Host;
                if !self.accepts_input(window) {
                    return None;
                }
                let editor = self.editor();
                let drawn = editor.drawn.borrow();
                Some($crate::path::field::ime_bounds(
                    &drawn.line,
                    &editor.entry,
                    &range,
                    drawn.bounds?,
                ))
            }

            fn character_index_for_point(
                &mut self,
                at: gpui::Point<gpui::Pixels>,
                window: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> Option<usize> {
                use $crate::g2::st1::input::Host;
                if !self.accepts_input(window) {
                    return None;
                }
                let editor = self.editor();
                let drawn = editor.drawn.borrow();
                Some($crate::path::field::ime_index(
                    &drawn.line,
                    &editor.entry,
                    at.x - drawn.bounds?.left(),
                ))
            }
        }
    };
}

pub(crate) use input_handler;
