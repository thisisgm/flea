//! Committed query edits go through the existing filter controller in one transaction.
use super::picker::Picker;
use gpui::{Context, Window};
use std::cell::RefCell;

impl Picker {
    pub(super) fn find_changed(&mut self, cx: &mut Context<Self>) {
        if self.find.entry.marked.is_some() {
            self.find_refresh(cx);
            cx.notify();
            return;
        }
        let text = self.find.entry.text.clone();
        if self.find.owner.query() == text {
            self.find_refresh(cx);
            cx.notify();
            return;
        }
        self.find_update(
            |filter, pane| {
                let common = filter
                    .query()
                    .chars()
                    .zip(text.chars())
                    .take_while(|(a, b)| a == b)
                    .map(|(ch, _)| ch.len_utf8())
                    .sum::<usize>();
                let remove = filter.query()[common..].chars().count();
                let mut asked = None;
                for _ in 0..remove {
                    asked = filter.backspace(pane);
                }
                if common < text.len() {
                    asked = filter.typed(&text[common..], pane);
                }
                filter.start();
                asked
            },
            cx,
        );
    }
}

impl super::text::EntryHost for Picker {
    fn entry(&self) -> &crate::path::entry::Entry {
        &self.find.entry
    }
    fn entry_mut(&mut self) -> &mut crate::path::entry::Entry {
        &mut self.find.entry
    }
    fn drawn(&self) -> &RefCell<super::text::Drawn> {
        &self.find.drawn
    }
    fn focus(&self) -> &gpui::FocusHandle {
        &self.focus
    }
    fn active(&self, window: &Window, _: &gpui::App) -> bool {
        !self.answered()
            && self.find.owner.typing()
            && self.find.entry.editing
            && self.focus.is_focused(window)
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.find_changed(cx);
    }
}
