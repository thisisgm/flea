//! The production pane owns Filter040: its query line, the backend's `filtered` answer, the strip slot and the rows List and Grid draw.
use crate::{
    g2::sr2::{
        filter::{Asked, Drawn, Filter, LineKey, PaneSeam},
        keys::{self, Classified},
        present,
        strip::{self, StripState},
        wire::{self, Line},
        StripView,
    },
    list::pane::Pane,
    state::Shell,
};
use gpui::{AppContext, Context, Entity, KeyDownEvent, Window};
use std::{cell::RefCell, rc::Rc};

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "filter_views_tests.rs"]
mod views_tests;

#[cfg(test)]
#[path = "fixfilter_tests.rs"]
mod fixfilter_tests;

// ui/js/Focus.js LEAVES_LINE and the pane's own moves: under a filter each one steps through the drawn rows.
const MOVES: [&str; 6] = [
    "cursorDown",
    "cursorUp",
    "cursorFirst",
    "cursorLast",
    "pageDown",
    "pageUp",
];

pub(crate) struct Runtime {
    pub owner: Filter,
    pub strip: Rc<RefCell<StripState>>,
    view: Option<Entity<StripView>>,
    input_generation: Option<u64>,
    // The answer the rows below were cut from, so a frame at rest rebuilds nothing.
    drawn: Option<Rc<Drawn>>,
    /// The drawn rows as one shared slice, None while every row is drawn.
    pub shown: Option<Rc<[usize]>>,
    /// Filter.js filterKeyAt and filterNarrowedAt: wall-clock ms of the last query change and of its answer landing.
    pub key_at: u64,
    pub narrowed_at: u64,
    /// The query an Enter committed while its answer was still out; the row acts when that answer lands.
    pub enter_after: Option<String>,
}

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |since| since.as_millis().max(1) as u64)
}

impl Runtime {
    pub fn new(face: crate::list::Typeface) -> Self {
        Self {
            owner: Filter::default(),
            strip: Rc::new(RefCell::new(StripState::new(face))),
            view: None,
            input_generation: None,
            drawn: None,
            shown: None,
            key_at: 0,
            narrowed_at: 0,
            enter_after: None,
        }
    }

    /// How many positions a view draws.
    pub fn shown_total(&self, total: usize) -> usize {
        self.shown.as_ref().map_or(total, |rows| rows.len())
    }

    // The drawn slice follows the answer in force; a new answer is copied once, never per frame.
    fn sync(&mut self, listing: usize) {
        let drawn = self.owner.drawn(listing).cloned();
        if drawn.as_ref().map(Rc::as_ptr) != self.drawn.as_ref().map(Rc::as_ptr) {
            self.shown = drawn.as_ref().map(|drawn| Rc::from(drawn.rows()));
            self.drawn = drawn;
        }
    }
}

/// The real pane behind `PaneSeam`: its cursor, its scroll in view positions and the pane's marks.
struct Seam<'a> {
    pane: &'a mut Pane,
    selected: &'a mut Vec<usize>,
}

impl PaneSeam for Seam<'_> {
    fn listing(&self) -> usize {
        self.pane.held.listing
    }
    fn total(&self) -> usize {
        self.pane.held.total
    }
    fn cursor(&self) -> Option<usize> {
        (self.pane.held.total > 0).then_some(self.pane.cursor)
    }
    fn set_cursor(&mut self, row: usize) {
        self.pane.cursor = row;
    }
    fn show_view(&mut self, view: usize) {
        self.pane.reveal_view(view);
    }
    fn selected(&self) -> Vec<usize> {
        self.selected.clone()
    }
    fn unmark(&mut self, rows: &[usize]) {
        let mut gone = rows.to_vec();
        gone.sort_unstable();
        self.selected.retain(|row| gone.binary_search(row).is_err());
    }
    fn mark(&mut self, rows: &[usize]) {
        self.selected.extend_from_slice(rows);
        self.selected.sort_unstable();
        self.selected.dedup();
    }
    // The list takes the keyboard back by the filter's own typing flag, which every key route reads.
    fn focus_list(&mut self) {}
    fn refill(&mut self) {
        self.pane.requests.initial_first = None;
        // The last Columns frame may still describe only the filtered rows.
        self.pane.scroll_extent = None;
        self.pane.reveal_cursor();
    }
}

impl crate::window::pane::PaneHost {
    /// A new numbering: a new folder drops the filter, a re-read or sort of this one asks the standing query again.
    pub(crate) fn filter_follow(&mut self, before: &str) {
        let mut seam = Seam {
            pane: &mut self.pane,
            selected: &mut self.selected,
        };
        let asked = if before != seam.pane.path() {
            self.filter.owner.abandon(&mut seam);
            None
        } else {
            self.filter.owner.renumbered(&mut seam)
        };
        self.filter.sync(self.pane.held.listing);
        self.input.set_shown(self.filter.shown.clone());
        if let (Some(asked), Some(backend)) = (asked, &self.backend) {
            let _ = backend.line(wire::request(asked.id, &asked.query).trim_end().to_owned());
        }
    }
}

impl Shell {
    /// One change to a pane's filter: its request goes out, then the drawn rows, the strip and the status follow it.
    pub(crate) fn filter_update(
        &mut self,
        side: usize,
        change: impl FnOnce(&mut Filter, &mut dyn PaneSeam) -> Option<Asked>,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        let query = host.filter.owner.query().to_owned();
        let listing = host.pane.held.listing;
        let before = host.filter.owner.drawn(listing).cloned();
        let filtered = host.filter.shown.clone();
        let asked = change(
            &mut host.filter.owner,
            &mut Seam {
                pane: &mut host.pane,
                selected: &mut host.selected,
            },
        );
        // Seam::refill reveals the list's cursor; the Grid keeps its own scroll, so a clear hands it over here.
        if let Some(filtered) =
            filtered.filter(|_| before.is_some() && host.filter.owner.drawn(listing).is_none())
        {
            host.grid_filter_cleared(&filtered);
        }
        // The seam marks and prunes host.selected directly, so the plate's snapshot follows it here.
        host.input.selection.sync(&host.selected);
        let owner = &host.filter.owner;
        if owner.query() != query {
            if let Some(backend) = &host.backend {
                host.sizes.cancel(backend);
            }
            host.filter.key_at = wall_ms();
        }
        let after = owner.drawn(listing);
        let same =
            matches!((&before, after), (Some(before), Some(after)) if Rc::ptr_eq(before, after));
        if after.is_some() && !same {
            host.filter.narrowed_at = wall_ms().max(host.filter.key_at);
        }
        if let (Some(asked), Some(backend)) = (asked, &host.backend) {
            if let Err(error) =
                backend.line(wire::request(asked.id, &asked.query).trim_end().to_owned())
            {
                host.stopped = true;
                host.record_error(error, &self.locale);
            }
        }
        self.filter_refresh(side, cx);
        cx.notify();
    }

    // The strip slot, its words, the status count and the rows views read, all from the answer in force.
    fn filter_refresh(&mut self, side: usize, cx: &mut Context<Self>) {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        let listing = host.pane.held.listing;
        host.filter.owner.sync_entry();
        host.filter.sync(listing);
        host.input.set_shown(host.filter.shown.clone());
        let owner = &host.filter.owner;
        if !owner.up() {
            self.slots.strips[side] = None;
            return;
        }
        let folder = std::path::Path::new(&host.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("/");
        let mut strip = host.filter.strip.borrow_mut();
        strip.text = present::strip(owner, listing, folder, &self.locale.catalog);
        let entry = owner.entry.borrow();
        if owner.typing() {
            strip.text.query = entry.text.clone().into();
        }
        if let Some(generation) = entry.session() {
            if host.filter.input_generation != Some(generation) {
                let input = crate::g2::sr2::input::FilterInput::new(
                    cx.weak_entity(),
                    side,
                    generation,
                    owner.entry.clone(),
                    strip.query.line.clone(),
                    strip.query.bounds.clone(),
                );
                strip.input = Some(cx.new(|_| input));
                host.filter.input_generation = Some(generation);
            }
        } else {
            strip.input = None;
            host.filter.input_generation = None;
        }
        drop(entry);
        drop(strip);
        if let Some(counts) = owner.counts(listing) {
            host.status = present::status(&counts, &self.locale.catalog).into();
        }
        let state = Rc::clone(&host.filter.strip);
        let view = host
            .filter
            .view
            .get_or_insert_with(|| cx.new(|_| StripView { state }))
            .clone();
        view.update(cx, |_, cx| cx.notify());
        self.slots.strips[side] = Some(view.into());
        self.slots.strip_height = strip::HEIGHT;
    }

    /// Whether "/" or a standing filter owns this action on the focused pane, ahead of the pane's own moves.
    pub(crate) fn filter_claims(&self, action: &str, cx: &gpui::App) -> bool {
        if self.input_blocked(cx) {
            return false;
        }
        let Some(host) = self.panes[self.focus_side].as_ref() else {
            return false;
        };
        if host.search.owner.active() || host.pane.state.search {
            return false;
        }
        let drawn = host.filter.owner.drawn(host.pane.held.listing).is_some();
        match action {
            "search" => host.filter.owner.up(),
            "filter" => true,
            "selectAll" => drawn,
            _ => MOVES.contains(&action) && drawn,
        }
    }

    /// "/" and the actions a standing filter owns: steps through the drawn rows and Ctrl+A over its exact rows.
    pub(crate) fn filter_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.filter_claims(action, cx) {
            return false;
        }
        let side = self.focus_side;
        let Some(host) = self.panes[side].as_mut() else {
            return false;
        };
        match action {
            // Search takes the pane: a standing filter closes first, and search goes on.
            "search" => {
                self.filter_update(
                    side,
                    |filter, pane| {
                        filter.close(pane);
                        None
                    },
                    cx,
                );
                self.search_action(action, window, cx)
            }
            "filter" => {
                if host.pane.busy() || host.stopped {
                    return true;
                }
                self.filter_update(
                    side,
                    |filter, _| {
                        filter.start();
                        None
                    },
                    cx,
                );
                window.focus(&self.focus, cx);
                true
            }
            "selectAll" => {
                self.filter_update(
                    side,
                    |filter, pane| {
                        filter.select_all(pane);
                        None
                    },
                    cx,
                );
                true
            }
            _ => {
                let page = (host.pane.visible_rows() / 2).max(1) as isize;
                self.filter_update(
                    side,
                    |filter, pane| {
                        match action {
                            "cursorDown" => filter.step(1, pane),
                            "cursorUp" => filter.step(-1, pane),
                            "pageDown" => filter.step(page, pane),
                            "pageUp" => filter.step(-page, pane),
                            "cursorFirst" => filter.set_cursor_view(0, pane),
                            _ => filter.set_cursor_view(isize::MAX, pane),
                        }
                        None
                    },
                    cx,
                );
                if let Some(host) = self.panes[side].as_mut() {
                    host.input
                        .selection
                        .follow(&mut host.selected, host.pane.cursor);
                    host.input.keyboard();
                }
                true
            }
        }
    }
}

#[path = "filter_input.rs"]
mod input;
pub(crate) use input::plain_enter;
