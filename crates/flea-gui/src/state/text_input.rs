//! Text-input ownership at the window root: the field the platform's text input reaches holds the keyboard while it composes, and lets go of its composition when focus leaves it.
use crate::{
    g2::{file_search::input::FindRoute, listing_filter::input::FilterRoute},
    state::Shell,
    strips::query_input::Route,
};
use gpui::{Context, EntityInputHandler, Window};

/// A field whose platform handler rides the shell's own focus, so its keys reach `Shell::key` after the field's own refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// The shell's adapter: the inline name and every dialog field its shared editor serves.
    Name,
    Filter(usize),
    Find(usize),
}

impl Shell {
    /// The field holding marked text now, read the way its adapter answers the platform: only while the shell holds focus.
    pub(crate) fn text_input_owner(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Owner> {
        if !self.focus.is_focused(window) {
            return None;
        }
        if self.marked_text_range(window, cx).is_some() {
            return Some(Owner::Name);
        }
        // A query's adapter answers nothing under a card, as `QueryInput::focused` reads it.
        if self.input_blocked(cx) {
            return None;
        }
        let side = self.focus_side;
        let host = self.panes[side].as_ref()?;
        if host.search.owner.typing() {
            return host
                .search
                .owner
                .entry
                .borrow()
                .marked
                .is_some()
                .then_some(Owner::Find(side));
        }
        (host.filter.owner.typing()
            && !host.search.owner.active()
            && host.filter.owner.entry.borrow().marked.is_some())
        .then_some(Owner::Filter(side))
    }

    /// Called by every key before the key reads the owner, and by every root frame, so a composition owns the keyboard exactly while its field does.
    pub(crate) fn sync_text_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owner = self.text_input_owner(window, cx);
        let previous = self.keyboard.text_owner();
        if owner == previous {
            return;
        }
        if let Some(previous) = previous {
            self.detach_text_input(previous, window, cx);
        }
        self.keyboard.own_text(owner);
    }

    /// The previous owner either committed, which leaves no mark, or lost focus mid-composition: its marked text is committed as shown, as a press does.
    fn detach_text_input(&mut self, owner: Owner, window: &mut Window, cx: &mut Context<Self>) {
        match owner {
            // A dialog can open or close with the loss, so both entries the shell's adapter serves let go.
            Owner::Name => {
                if self.rename.ime.entry.ime_unmark() {
                    self.dialog_edited(cx);
                }
                if let Some(edit) = self.rename.edit.as_mut() {
                    if edit.entry.ime_unmark() {
                        cx.notify();
                    }
                }
            }
            Owner::Filter(side) => {
                if let Some(generation) = self.query_unmark(side, false) {
                    FilterRoute::commit(self, side, generation, window, cx);
                }
            }
            Owner::Find(side) => {
                if let Some(generation) = self.query_unmark(side, true) {
                    FindRoute::commit(self, side, generation, window, cx);
                }
            }
        }
    }

    // The strip's entry loses its mark; the generation it was typed in is the commit's.
    fn query_unmark(&mut self, side: usize, find: bool) -> Option<u64> {
        let host = self.panes[side].as_ref()?;
        let entry = if find {
            &host.search.owner.entry
        } else {
            &host.filter.owner.entry
        };
        let mut entry = entry.borrow_mut();
        entry.ime_unmark().then_some(entry.generation)
    }
}
