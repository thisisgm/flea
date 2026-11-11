//! The Find entry edits locally; only committed text reaches the existing search session.
use super::{Host, Search, StripKey};
use crate::strips::query_input::{
    cancel_composition, follow, move_caret, reopen, settled, CANDIDATE_KEYS,
};
use gpui::KeyDownEvent;

impl Search {
    pub(super) fn sync_entry(&mut self) {
        let typing = self.typing();
        follow(&mut self.entry.borrow_mut(), typing, &self.session.query);
    }

    /// A retained platform callback must still name the same open field.
    pub fn commit_query(&mut self, generation: u64, host: &mut dyn Host) {
        let Some(text) = settled(
            &self.entry.borrow(),
            self.typing(),
            generation,
            &self.session.query,
        ) else {
            return;
        };
        self.session.query.clear();
        let effects = self.session.extend(&host.pane().into(), &text);
        self.apply(effects, host);
    }

    pub(super) fn query_key(&mut self, key: &StripKey, host: &mut dyn Host) -> bool {
        self.sync_entry();
        if !self.typing() {
            return false;
        }
        let mut entry = self.entry.borrow_mut();
        if entry.marked.is_some() {
            match key {
                StripKey::Contents if !self.session.wire.contents => return true,
                StripKey::Escape => cancel_composition(&mut entry, &self.session.query),
                StripKey::Tab | StripKey::Contents => {
                    entry.ime_unmark();
                    self.session.query.clone_from(&entry.text);
                    reopen(&mut entry, &self.session.query);
                    return false;
                }
                _ => {}
            }
            return true;
        }
        match key {
            StripKey::Text(text) if !text.is_empty() && !text.chars().any(char::is_control) => {
                entry.ime_replace(None, text);
            }
            StripKey::Paste => {
                let text =
                    crate::window::input::clean_paste(&host.clipboard_text().unwrap_or_default());
                if text.is_empty() {
                    return true;
                }
                entry.ime_replace(None, &text);
            }
            StripKey::Backspace => entry.erase(false),
            _ => return false,
        }
        let generation = entry.generation;
        drop(entry);
        self.commit_query(generation, host);
        true
    }

    /// Entry navigation stays in Find; candidate arrows never move the results beneath it.
    pub fn key_event(&mut self, event: &KeyDownEvent, host: &mut dyn Host) {
        let keys = &event.keystroke;
        let held = keys.modifiers;
        let mut entry = self.entry.borrow_mut();
        let key = keys.key.as_str();
        let command = held.control || held.platform;
        if entry.marked.is_some() && CANDIDATE_KEYS.contains(&key) {
            return;
        }
        if !held.alt && !command && move_caret(&mut entry, key, held.shift) {
            return;
        }
        if key == "a" && command && !held.alt && !held.shift && entry.marked.is_none() {
            entry.select_all();
            return;
        }
        if key == "delete" && !held.alt && !command && entry.marked.is_none() {
            entry.erase(true);
            let generation = entry.generation;
            drop(entry);
            self.commit_query(generation, host);
            return;
        }
        drop(entry);
        self.key(StripKey::from_event(event), host);
    }

    /// Scope changes commit a composition into that scope's single existing generation.
    pub(super) fn commit_scope_draft(&mut self) {
        let mut entry = self.entry.borrow_mut();
        if entry.ime_unmark() {
            self.session.query.clone_from(&entry.text);
            reopen(&mut entry, &self.session.query);
        }
    }
}
