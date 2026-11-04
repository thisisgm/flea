//! Preview Copy and Escape keep the listing's focus; without a range every key keeps its table's meaning.
use crate::{g2::pv2::selection, state::Shell};
use flea::keys::Context as Keys;
use gpui::{Context, KeyDownEvent, MouseButton, MouseDownEvent, Window};

impl Shell {
    pub(crate) fn key_with_preview(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.column_keys_follow_focus(window, cx);
        let eligible = !self.card_holds_keyboard(cx)
            && !self.menu_opened()
            && self.rename.edit.is_none()
            && !self.path_holds_keyboard(window, cx);
        if eligible && selection::key(event, window, cx) {
            return;
        }
        if !eligible {
            selection::clear_window(window, cx);
        }
        let look = self.quick_look_open(cx);
        self.key(event, window, cx);
        if look != self.quick_look_open(cx) {
            selection::clear_window(window, cx);
        }
    }

    /// GPUI moves focus on an outside press without changing the key table; route the next key to the listing.
    fn column_keys_follow_focus(&mut self, window: &Window, cx: &mut Context<Self>) {
        let column = matches!(
            self.keyboard.context,
            Keys::Preview | Keys::Pdf | Keys::Media
        );
        if !column || self.quick_look_open(cx) {
            return;
        }
        let focused = (0..2)
            .filter_map(|side| self.selection_preview(side))
            .any(|preview| preview.read(cx).focus.is_focused(window));
        if !focused {
            self.keyboard.context = Keys::Listing;
        }
    }

    /// A middle press may close a tab; a left press on a column preview takes the keyboard back from the path bar.
    pub(crate) fn capture_press(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.middle_tab_pointer(event, window, cx);
        if event.button != MouseButton::Left || !self.path_holds_keyboard(window, cx) {
            return;
        }
        let on_preview = (0..2)
            .filter_map(|side| self.selection_preview(side))
            .any(|preview| {
                let preview = preview.read(cx);
                preview.visible
                    && preview
                        .frame_rect
                        .borrow()
                        .is_some_and(|rect| rect.contains(&event.position))
            });
        if on_preview {
            window.focus(&self.focus, cx);
        }
    }
}
