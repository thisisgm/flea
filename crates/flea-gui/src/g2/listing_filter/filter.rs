//! One pane's filter: the typed query, the answer it draws, and every transition ui/js/Filter.js and Filter040 define.
use super::wire::Answer;
use crate::strips::query_input;
use flea::ui_logic_listing_filter::{self as logic, Counts, Narrowed, Refusal, Scope};
use std::rc::Rc;

/// What the filter needs of the pane it narrows; G1 implements it on the real pane, tests on a stub.
pub trait PaneSeam {
    /// The listing stamp in force, the numbering every answer must name.
    fn listing(&self) -> usize;
    fn total(&self) -> usize;
    fn cursor(&self) -> Option<usize>;
    fn set_cursor(&mut self, row: usize);
    /// Scroll so a view position is on screen, Pane.qml's showRow.
    fn show_view(&mut self, view: usize);
    fn selected(&self) -> Vec<usize>;
    fn unmark(&mut self, rows: &[usize]);
    /// Ctrl+A on the query line: these rows become the selection.
    fn mark(&mut self, rows: &[usize]);
    /// The list takes the keyboard back from the query line.
    fn focus_list(&mut self);
    /// After a clear: refill the held window the unfiltered viewport draws, issue 315's refill.
    fn refill(&mut self);
}

/// A request in flight; only its answer may land.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    pub id: u64,
    pub query: String,
    pub listing: usize,
}

/// The answer a view draws, swapped whole when the next one lands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Drawn {
    pub query: String,
    pub listing: usize,
    pub tested: usize,
    pub narrowed: Narrowed,
}

impl Drawn {
    /// The rows a view draws: the exact ones, or with none the close ones in rank order.
    pub fn rows(&self) -> &[usize] {
        if self.narrowed.exact.is_empty() {
            &self.narrowed.close
        } else {
            &self.narrowed.exact
        }
    }
}

/// A key the query line classified; see `keys::classify`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKey<'a> {
    Text(&'a str),
    Enter,
    Escape,
    Backspace,
    Tab,
    SelectAll,
    /// A cursor key: the line commits and the key moves the list.
    Leaves,
    Other,
}

/// Why an answer was not drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Not the request in force: an older id or query, a cleared filter, or a numbering that moved on.
    Stale,
}

#[derive(Default)]
pub struct Filter {
    pub entry: Rc<std::cell::RefCell<crate::path::entry::Entry>>,
    query: String,
    typing: bool,
    next_id: u64,
    // Every keystroke's request still awaited, oldest first; Filter.js narrows on each one.
    asked: Vec<Asked>,
    drawn: Option<Rc<Drawn>>,
}

impl Filter {
    /// Keeps the draft session with the line while the committed query remains the answer's identity.
    pub(crate) fn sync_entry(&mut self) {
        query_input::follow(&mut self.entry.borrow_mut(), self.typing, &self.query);
    }

    /// One composed commit goes through the existing request and listing-generation path.
    pub(crate) fn commit_query(
        &mut self,
        generation: u64,
        pane: &mut dyn PaneSeam,
    ) -> Option<Asked> {
        self.query =
            query_input::settled(&self.entry.borrow(), self.typing, generation, &self.query)?;
        self.ask(pane)
    }

    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn typing(&self) -> bool {
        self.typing
    }
    /// The newest request in flight.
    pub fn asked(&self) -> Option<&Asked> {
        self.asked.last()
    }
    /// The oldest id still awaited, for `wire::read`; None while nothing is asked.
    pub fn oldest(&self) -> Option<u64> {
        self.asked.first().map(|asked| asked.id)
    }
    /// FilterStrip's `up`: the line has the caret or a query stands.
    pub fn up(&self) -> bool {
        self.typing || !self.query.is_empty()
    }

    /// The answer in force for this numbering; None with no query, or while an answer for a new numbering is out.
    pub fn drawn(&self, listing: usize) -> Option<&Rc<Drawn>> {
        self.drawn
            .as_ref()
            .filter(|drawn| !self.query.is_empty() && drawn.listing == listing)
    }

    /// The match list views read, None meaning the identity.
    pub fn shown(&self, listing: usize) -> Option<&[usize]> {
        self.drawn(listing).map(|drawn| drawn.rows())
    }

    /// The strip's numbers, once an answer for this numbering stands; while a newer query is out they stay the drawn answer's.
    pub fn counts(&self, listing: usize) -> Option<Counts> {
        self.drawn(listing)
            .map(|drawn| Counts::of(&drawn.narrowed, drawn.tested, Scope::Folder))
    }

    /// "/" gives the query line the keyboard; nothing narrows until something is typed.
    pub fn start(&mut self) {
        self.typing = true;
        self.sync_entry();
    }

    fn ask(&mut self, pane: &mut dyn PaneSeam) -> Option<Asked> {
        if self.query.is_empty() {
            self.clear(pane);
            return None;
        }
        self.next_id += 1;
        let asked = Asked {
            id: self.next_id,
            query: self.query.clone(),
            listing: pane.listing(),
        };
        self.asked.push(asked.clone());
        Some(asked)
    }

    // The match list goes and the unfiltered viewport refills; marks the filter already pruned stay as they are.
    fn clear(&mut self, pane: &mut dyn PaneSeam) {
        self.asked.clear();
        if self.drawn.take().is_some() {
            pane.refill();
        }
    }

    /// One keystroke or one paste narrows by its text; a control character types nothing.
    pub fn typed(&mut self, text: &str, pane: &mut dyn PaneSeam) -> Option<Asked> {
        let clean = crate::window::input::clean_paste(text);
        if clean.is_empty() {
            return None;
        }
        if self.entry.borrow().marked.is_some() {
            return None;
        }
        self.typing = true;
        self.sync_entry();
        let generation = {
            let mut entry = self.entry.borrow_mut();
            entry.replace(None, &clean);
            entry.generation
        };
        self.commit_query(generation, pane)
    }

    pub fn backspace(&mut self, pane: &mut dyn PaneSeam) -> Option<Asked> {
        if !self.typing {
            self.query.pop()?;
            return self.ask(pane);
        }
        self.sync_entry();
        let generation = {
            let mut entry = self.entry.borrow_mut();
            entry.erase(false);
            entry.generation
        };
        self.commit_query(generation, pane)
    }

    /// Enter hands the keyboard to the list with the filter standing; an empty query has nothing to stand, so it closes.
    pub fn commit(&mut self, pane: &mut dyn PaneSeam) {
        if self.query.is_empty() {
            self.close(pane);
            return;
        }
        self.typing = false;
        self.entry.borrow_mut().close();
        pane.focus_list();
    }

    /// Escape and every fresh folder: the query, the line and the request in flight all go.
    pub fn close(&mut self, pane: &mut dyn PaneSeam) {
        let was = self.up();
        self.query.clear();
        self.typing = false;
        self.entry.borrow_mut().close();
        self.clear(pane);
        if was {
            pane.focus_list();
        }
    }

    /// The listing's Escape: true when the filter took it, which it does first, ahead of search and marks.
    pub fn escape(&mut self, pane: &mut dyn PaneSeam) -> bool {
        if !self.up() {
            return false;
        }
        self.close(pane);
        true
    }

    /// The list's Ctrl+A over a standing filter: marks exact rows and focuses the list; an empty query marks nothing.
    pub fn select_all(&mut self, pane: &mut dyn PaneSeam) {
        if self.query.is_empty() {
            return;
        }
        let exact = self
            .drawn(pane.listing())
            .map(|drawn| drawn.narrowed.exact.clone())
            .unwrap_or_default();
        if !exact.is_empty() {
            pane.mark(&exact);
        }
        self.typing = false;
        self.entry.borrow_mut().close();
        if self.query.is_empty() {
            self.close(pane);
        } else {
            pane.focus_list();
        }
    }

    /// The query line's own keys while it has the caret; a key that leaves the line commits first and is not consumed.
    pub fn line_key(&mut self, key: LineKey<'_>, pane: &mut dyn PaneSeam) -> (bool, Option<Asked>) {
        if self.entry.borrow().marked.is_some() {
            if matches!(key, LineKey::Escape) {
                query_input::cancel_composition(&mut self.entry.borrow_mut(), &self.query);
            }
            return (true, None);
        }
        match key {
            LineKey::Text(text) => (true, self.typed(text, pane)),
            LineKey::Backspace => (true, self.backspace(pane)),
            LineKey::Enter => {
                self.commit(pane);
                (true, None)
            }
            LineKey::Escape => {
                self.close(pane);
                (true, None)
            }
            // Tab's subfolder scope waits on the joined recursive producer (sr1's searchopen and searchquery), so it is consumed and inert.
            LineKey::Tab => (true, None),
            // EY-115: v0.3.12's Filter.js typeKey swallows Ctrl+A, so the line keeps its caret, query and marks.
            LineKey::SelectAll => (true, None),
            LineKey::Leaves => {
                self.commit(pane);
                (false, None)
            }
            LineKey::Other => (true, None),
        }
    }

    /// An answer to any awaited keystroke lands whole or not at all, in order, retiring the requests before it.
    /// Marks it hides drop out, and a hidden cursor takes the first row standing.
    pub fn receive(&mut self, answer: Answer, pane: &mut dyn PaneSeam) -> Result<(), Refused> {
        let at = self
            .asked
            .iter()
            .position(|asked| asked.id == answer.id)
            .ok_or(Refused::Stale)?;
        let asked = &self.asked[at];
        if answer.query != asked.query
            || answer.listing != asked.listing
            || answer.listing != pane.listing()
        {
            return Err(Refused::Stale);
        }
        self.asked.drain(..=at);
        let drawn = Rc::new(Drawn {
            query: answer.query,
            listing: answer.listing,
            tested: answer.total,
            narrowed: Narrowed {
                exact: answer.exact.unwrap_or_default(),
                close: answer.close,
            },
        });
        let rows = drawn.rows();
        let hidden = logic::pruned(&pane.selected(), Some(rows));
        if !hidden.is_empty() {
            pane.unmark(&hidden);
        }
        if let Some(to) = pane
            .cursor()
            .and_then(|cursor| logic::cursor_after(Some(rows), cursor))
        {
            pane.set_cursor(to);
            pane.show_view(0);
        }
        self.drawn = Some(drawn);
        Ok(())
    }

    /// A sort or re-read renumbers the listing: a standing query asks again against the new numbering.
    pub fn renumbered(&mut self, pane: &mut dyn PaneSeam) -> Option<Asked> {
        if self.query.is_empty() {
            return None;
        }
        self.ask(pane)
    }

    /// A new folder, or a backend that stopped answering: nothing the old numbering said may land.
    pub fn abandon(&mut self, pane: &mut dyn PaneSeam) {
        self.close(pane);
    }

    /// One step through the drawn rows; the scroll goes to the step's view position.
    pub fn step(&self, delta: isize, pane: &mut dyn PaneSeam) {
        let Some(cursor) = pane.cursor() else { return };
        let list = self.shown(pane.listing());
        if let Some((row, view)) = logic::step(list, pane.total(), cursor, delta) {
            pane.set_cursor(row);
            pane.show_view(view);
        }
    }

    /// The cursor set to a view position, clamped to what is drawn.
    pub fn set_cursor_view(&self, view: isize, pane: &mut dyn PaneSeam) {
        let list = self.shown(pane.listing());
        if let Some((row, view)) = logic::cursor_at_view(list, pane.total(), view) {
            pane.set_cursor(row);
            pane.show_view(view);
        }
    }

    /// The rows an action takes, or why it takes none.
    pub fn targets(
        &self,
        pane: &dyn PaneSeam,
        loaded: impl Fn(usize) -> bool,
    ) -> Result<Vec<usize>, Refusal> {
        logic::targets(
            &pane.selected(),
            self.shown(pane.listing()),
            pane.cursor(),
            loaded,
        )
    }
}
