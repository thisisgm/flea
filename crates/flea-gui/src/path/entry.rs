//! The typed half of the path bar: the line, its selection and marked text, Tab's completion and the commit.
use super::{
    complete, completion_dir, completion_message, is_network_uri, refused, request_key, resolve,
    should_navigate, wants_hidden, Ask, Peeked, Said,
};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

mod ime;
mod word;
pub use ime::right_to_left;
use ime::{binds, extends};
#[cfg(test)]
mod tests;

// Session ids are unique in the process, so a reset Entry never reuses an id a retained callback holds.
static SESSIONS: AtomicU64 = AtomicU64::new(1);
const UNDO_LIMIT: usize = 64;

/// What a commit asks of the window once the bar has closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Commit {
    Open(String),
    Network(String),
    Refused,
    Nothing,
}

/// What Tab did: answered from the kept rows, or asked the backend for a peek.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tab {
    Answered(Option<Said>),
    Asked(Ask),
}

/// What a line accepts and shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Text,
    /// A password: the platform never reads its text, and paint shows one bullet per character.
    Private,
    /// A number: only ASCII digits enter the line.
    Digits,
}

#[derive(Default)]
pub struct Entry {
    pub editing: bool,
    pub text: String,
    pub selected: Range<usize>,
    pub reversed: bool,
    pub marked: Option<Range<usize>>,
    pending: Option<Ask>,
    cached: Option<(Ask, Vec<String>)>,
    /// Bumped by every change to the line or its selection, so a shaped line knows it is stale.
    pub revision: u64,
    /// The editing session this line belongs to; a later session never repeats it.
    pub generation: u64,
    pub kind: Kind,
    history: Vec<(String, Range<usize>, bool)>,
}

impl Entry {
    // The trailing slash makes a child the next keystroke, and the line opens selected so a typed name replaces it.
    pub fn start(&mut self, path: &str) -> bool {
        if self.editing {
            return false;
        }
        self.editing = true;
        self.text = if path == "/" {
            "/".into()
        } else {
            format!("{path}/")
        };
        self.selected = 0..self.text.len();
        self.reversed = false;
        self.marked = None;
        self.history.clear();
        self.revision += 1;
        self.generation = SESSIONS.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// A new editing session on `text`, all of it selected; callbacks held for an earlier session stop matching.
    pub fn begin(&mut self, text: &str) {
        *self = Entry {
            editing: true,
            text: text.into(),
            kind: self.kind,
            revision: self.revision + 1,
            generation: SESSIONS.fetch_add(1, Ordering::Relaxed),
            ..Entry::default()
        };
        self.select_all();
    }

    /// The live session's id, None once the line has closed.
    pub fn session(&self) -> Option<u64> {
        self.editing.then_some(self.generation)
    }

    pub fn close(&mut self) -> bool {
        if !self.editing {
            return false;
        }
        self.editing = false;
        self.pending = None;
        self.cached = None;
        self.marked = None;
        self.history.clear();
        if self.kind == Kind::Private {
            self.text.clear();
            self.selected = 0..0;
        }
        self.revision += 1;
        true
    }

    /// Tab: the kept rows answer a repeat; anything else asks the backend's peek, the read the columns already make.
    pub fn complete(&mut self, current: &str, home: &str, show_hidden: bool) -> Tab {
        let dir = completion_dir(&self.text, current, home);
        let ask = request_key(&dir, wants_hidden(&self.text, show_hidden));
        if let Some((kept, names)) = self.cached.take() {
            if kept == ask {
                let said = self.apply(&dir, &names);
                self.cached = Some((kept, names));
                return Tab::Answered(said);
            }
            self.cached = Some((kept, names));
        }
        self.pending = Some(ask.clone());
        Tab::Asked(ask)
    }

    /// Whether a peeked reply answers this entry's own outstanding ask.
    pub fn asked(&self, peeked: &Peeked) -> bool {
        self.pending.as_ref() == Some(&request_key(&peeked.path, peeked.hidden))
    }

    /// A peek came back: only the asked (dir, hidden) pair is kept, and it applies only to an uncomposed line still reading that directory.
    pub fn complete_with(
        &mut self,
        peeked: &Peeked,
        current: &str,
        home: &str,
    ) -> Option<Option<Said>> {
        let ask = request_key(&peeked.path, peeked.hidden);
        if self.pending.as_ref() != Some(&ask) {
            return None;
        }
        self.pending = None;
        self.cached = Some((ask, peeked.dirs.clone()));
        if !self.editing
            || self.marked.is_some()
            || completion_dir(&self.text, current, home) != peeked.path
        {
            return None;
        }
        Some(self.apply(&peeked.path, &peeked.dirs))
    }

    pub fn pending(&self) -> Option<&Ask> {
        self.pending.as_ref()
    }

    fn apply(&mut self, dir: &str, names: &[String]) -> Option<Said> {
        let before = std::mem::take(&mut self.text);
        let after = complete(&before, names);
        let said = completion_message(&before, &after, dir);
        self.text = after.text;
        self.move_to(self.text.len());
        said
    }

    /// Enter on the line: the bar closes, and a settled same path stays a no-op that keeps the selection.
    pub fn commit(&mut self, current: &str, home: &str, failed: bool) -> Commit {
        // The line stays in the field after it closes, as the seam reads it.
        let typed = self.text.clone();
        let target = resolve(&typed, current, home);
        self.close();
        if should_navigate(&target, current, failed) {
            if is_network_uri(&target) {
                return Commit::Network(target);
            }
            return Commit::Open(target);
        }
        if refused(&typed) {
            return Commit::Refused;
        }
        Commit::Nothing
    }

    pub fn cursor(&self) -> usize {
        if self.reversed {
            self.selected.start
        } else {
            self.selected.end
        }
    }

    pub fn move_to(&mut self, offset: usize) {
        let offset = self.boundary(offset);
        self.selected = offset..offset;
        self.reversed = false;
        self.revision += 1;
    }

    pub fn select_to(&mut self, offset: usize) {
        let offset = self.boundary(offset);
        if self.reversed {
            self.selected.start = offset;
        } else {
            self.selected.end = offset;
        }
        if self.selected.end < self.selected.start {
            self.reversed = !self.reversed;
            self.selected = self.selected.end..self.selected.start;
        }
        self.revision += 1;
    }

    pub fn select_all(&mut self) {
        self.selected = 0..self.text.len();
        self.reversed = false;
        self.revision += 1;
    }

    fn boundary(&self, offset: usize) -> usize {
        let offset = offset.min(self.text.len());
        (0..=offset)
            .rev()
            .find(|at| self.text.is_char_boundary(*at))
            .unwrap_or(0)
    }

    /// The start of the cluster before offset: a base character with the marks and joiners riding on it.
    pub fn previous(&self, offset: usize) -> usize {
        let mut chars = self.text[..self.boundary(offset)]
            .char_indices()
            .rev()
            .peekable();
        let mut at = 0;
        while let Some((index, ch)) = chars.next() {
            at = index;
            if !extends(ch) && !chars.peek().is_some_and(|&(_, before)| binds(before, ch)) {
                break;
            }
        }
        at
    }

    /// The end of the cluster at offset.
    pub fn next(&self, offset: usize) -> usize {
        let offset = self.boundary(offset);
        let mut chars = self.text[offset..].char_indices().peekable();
        let Some((_, first)) = chars.next() else {
            return offset;
        };
        let (mut end, mut last) = (offset + first.len_utf8(), first);
        while let Some((index, ch)) = chars.next_if(|&(_, ch)| extends(ch) || binds(last, ch)) {
            (end, last) = (offset + index + ch.len_utf8(), ch);
        }
        end
    }

    /// Backspace takes one character, so a mark goes on its own as GTK does; Delete takes the cluster.
    pub fn erase(&mut self, forward: bool) {
        if self.selected.is_empty() {
            let at = self.cursor();
            let to = if forward {
                self.next(at)
            } else {
                self.text[..at]
                    .char_indices()
                    .next_back()
                    .map_or(0, |(at, _)| at)
            };
            if to == at {
                return;
            }
            self.select_to(to);
        }
        self.replace(None, "");
    }

    pub fn replace(&mut self, range: Option<Range<usize>>, text: &str) {
        self.remember();
        let range = self.target(range);
        self.text.replace_range(range.clone(), text);
        let end = range.start + text.len();
        self.selected = end..end;
        self.reversed = false;
        self.marked = None;
        self.revision += 1;
    }

    /// The input method's composing text, underlined until it commits.
    pub fn replace_and_mark(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
    ) {
        self.remember();
        let range = self.target(range);
        self.text.replace_range(range.clone(), text);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        self.selected = selected
            .map(|inner| {
                self.boundary(range.start + inner.start)..self.boundary(range.start + inner.end)
            })
            .unwrap_or(range.start + text.len()..range.start + text.len());
        self.reversed = false;
        self.revision += 1;
    }

    fn remember(&mut self) {
        // A composition is one edit; only its first preedit saves the preceding line.
        if self.kind != Kind::Private && self.marked.is_none() {
            if self.history.len() == UNDO_LIMIT {
                self.history.remove(0);
            }
            self.history
                .push((self.text.clone(), self.selected.clone(), self.reversed));
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some((text, selected, reversed)) = self.history.pop() {
            self.text = text;
            self.selected = selected;
            self.reversed = reversed;
            self.marked = None;
            self.revision += 1;
        }
    }

    fn target(&self, range: Option<Range<usize>>) -> Range<usize> {
        let range = range
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selected.clone());
        self.boundary(range.start)..self.boundary(range.end.max(range.start))
    }

    pub fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (at, next) in self.text.char_indices() {
            if utf16 >= offset {
                return at;
            }
            utf16 += next.len_utf16();
        }
        self.text.len()
    }

    pub fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text[..self.boundary(offset)]
            .chars()
            .map(char::len_utf16)
            .sum()
    }

    pub fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    pub fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    /// The selected text a copy may take; a private line gives none.
    pub fn copyable(&self) -> Option<&str> {
        (self.kind != Kind::Private && !self.selected.is_empty())
            .then(|| &self.text[self.selected.clone()])
    }
}
