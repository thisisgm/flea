//! The filter's mounted platform adapter: the shared query input routed to this pane's filter.
use crate::{
    path::entry::Entry,
    state::Shell,
    strips::query_input::{self, move_caret, CANDIDATE_KEYS},
};
use gpui::{Context, Window};
use std::{cell::RefCell, rc::Rc};

pub enum FilterRoute {}

pub type FilterInput = query_input::QueryInput<FilterRoute>;

impl query_input::Route for FilterRoute {
    fn owns(shell: &Shell, side: usize, entry: &Rc<RefCell<Entry>>) -> bool {
        shell.panes[side].as_ref().is_some_and(|host| {
            host.filter.owner.typing()
                && !host.search.owner.active()
                && Rc::ptr_eq(&host.filter.owner.entry, entry)
        })
    }

    fn commit(
        shell: &mut Shell,
        side: usize,
        generation: u64,
        _: &mut Window,
        cx: &mut Context<Shell>,
    ) {
        shell.filter_update(
            side,
            |filter, pane| filter.commit_query(generation, pane),
            cx,
        );
    }

    fn redraw(shell: &mut Shell, side: usize, cx: &mut Context<Shell>) {
        shell.filter_update(side, |_, _| None, cx);
    }
}

impl Shell {
    /// Candidate arrows stay with the IME; ordinary horizontal movement edits this entry.
    pub(crate) fn filter_entry_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let side = self.focus_side;
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        let mut entry = host.filter.owner.entry.borrow_mut();
        let key = event.keystroke.key.as_str();
        if entry.marked.is_some() {
            return CANDIDATE_KEYS.contains(&key);
        }
        let held = event.keystroke.modifiers;
        if held.control || held.alt || held.platform {
            return false;
        }
        match key {
            "left" | "right" => {
                move_caret(&mut entry, key, held.shift);
                drop(entry);
                self.filter_update(side, |_, _| None, cx);
            }
            "delete" => {
                entry.erase(true);
                let generation = entry.generation;
                drop(entry);
                self.filter_update(
                    side,
                    |filter, pane| filter.commit_query(generation, pane),
                    cx,
                );
            }
            _ => return false,
        }
        true
    }
}
