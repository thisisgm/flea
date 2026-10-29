//! The Grid view: one tile Element over the pane's held window, with the scroll, landing and window policy of ui/GridArea.qml.
pub mod captions;
pub mod element;
#[cfg(test)]
mod fix1_tests;
#[cfg(test)]
mod fix2_tests;
#[cfg(test)]
mod fix3_tests;
pub mod geometry;
mod glide;
pub use glide::Glide;
mod zoom;
pub use zoom::{zoom_travel, ZoomInput};
pub(crate) mod host;
mod input;
#[cfg(test)]
mod join_landing_tests;
#[cfg(test)]
mod join_tests;
#[cfg(test)]
mod join_transition_tests;
#[cfg(test)]
mod join_window_tests;
pub mod layout;
#[cfg(test)]
mod pagekeys_tests;
pub(crate) mod pane_view;
#[cfg(test)]
mod rev_u10_r1_tests;
#[cfg(test)]
mod rev_u10_r2_tests;
#[cfg(test)]
mod rev_u10_r3_tests;
mod scrollbar;
mod tile;
mod transition;
#[cfg(test)]
mod wheel_tests;

use crate::pane::{requests::WindowRequest, Pane};
use geometry::{window_for, Density, Metrics, ThumbSize, WindowPlan};
use layout::Layout;
use std::ops::RangeInclusive;

/// The filter's view of the listing, Filter.js's shown list: every row, or the listing indices a narrowing kept in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown<'a> {
    All(usize),
    Only(&'a [usize]),
}

impl Shown<'_> {
    pub fn total(&self) -> usize {
        match self {
            Self::All(total) => *total,
            Self::Only(rows) => rows.len(),
        }
    }
    /// Filter.at: the listing row a view position draws, None past the end.
    pub fn at(&self, view: usize) -> Option<usize> {
        match self {
            Self::All(total) => (view < *total).then_some(view),
            Self::Only(rows) => rows.get(view).copied(),
        }
    }
    /// Filter.viewOf: None for a row the filter hides.
    pub fn view_of(&self, listing: usize) -> Option<usize> {
        match self {
            Self::All(total) => (listing < *total).then_some(listing),
            Self::Only(rows) => rows.iter().position(|&row| row == listing),
        }
    }
    /// Filter.span: the listing rows a run of view positions covers, None when nothing is shown.
    pub fn span(&self, first: usize, last: usize) -> Option<RangeInclusive<usize>> {
        let end = self.total().checked_sub(1)?;
        let (lo, hi) = (first.min(end), last.min(end));
        Some(self.at(lo)?..=self.at(hi)?)
    }
    pub fn narrowed(&self) -> bool {
        matches!(self, Self::Only(_))
    }
}

mod focus;
pub use focus::{FocusFacts, FocusSnapshot};

/// The four grid arrows of keys.toml; Page, Home and End are the shared cursor moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrow {
    Up,
    Down,
    Left,
    Right,
    ExtendUp,
    ExtendDown,
}

/// Grid.js arrow: the view position an arrow lands on, None when it is held at a row edge or a listing end, where the key is still taken.
pub fn arrow(columns: usize, view: Option<usize>, total: usize, step: Arrow) -> Option<usize> {
    let (view, columns) = (view?, columns.max(1));
    let next = match step {
        Arrow::Down | Arrow::ExtendDown => view.checked_add(columns)?,
        Arrow::Up | Arrow::ExtendUp => view.checked_sub(columns)?,
        Arrow::Left if view % columns == 0 => return None,
        Arrow::Left => view - 1,
        Arrow::Right => view + 1,
    };
    (next < total && !(step == Arrow::Right && next % columns == 0)).then_some(next)
}

/// Why the view moved or asked, counted so tests and the IPC seam can read the policy back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub drift_asks: usize,
    pub landing_asks: usize,
    pub reveals: usize,
}

/// The grid's own state beside the pane: size, zoom, scroll and the landing wait; rows, cursor and selection stay with their owners.
#[derive(Clone, Debug, PartialEq)]
pub struct GridView {
    pub metrics: Metrics,
    pub density: Density,
    pub thumb: ThumbSize,
    pub size: (f32, f32),
    pub scroll: f32,
    pub visible: bool,
    pub hidden_held: bool,
    pub landing_wait: bool,
    pub landing_start: Option<usize>,
    /// A far landing's window, held while another window is out; the pane's one slot is never overwritten.
    pub landing_held: Option<(usize, usize)>,
    pub rename_extra: f32,
    pub hovered: Option<usize>,
    /// The live pointer in window pixels, so a scroll re-tests the freshly drawn tile mask.
    pub pointer: Option<(f32, f32)>,
    pub zoom_travel: f32,
    pub counters: Counters,
    /// A wheel notch's OutCubic glide to `scroll`; paint draws its position, everything else reads `scroll`.
    pub glide: Option<Glide>,
}

impl GridView {
    pub fn new(metrics: Metrics, size: (f32, f32)) -> Self {
        Self {
            metrics,
            density: Density::default(),
            thumb: ThumbSize::MEDIUM,
            size,
            scroll: 0.,
            visible: true,
            hidden_held: false,
            landing_wait: false,
            landing_start: None,
            landing_held: None,
            rename_extra: 0.,
            hovered: None,
            pointer: None,
            zoom_travel: 0.,
            counters: Counters::default(),
            glide: None,
        }
    }

    pub fn layout(&self, shown: Shown) -> Layout {
        Layout::new(
            self.metrics,
            self.size,
            self.thumb,
            self.density,
            shown.total(),
            self.rename_extra,
        )
    }
    /// GridArea.qml windowPlan: the list's 150-row buffer in pixels rebuilt in tile rows.
    pub fn plan(&self, layout: &Layout) -> WindowPlan {
        use crate::pane::requests::{BUFFER, MARGIN};
        window_for(
            layout.columns,
            layout.visible_rows(),
            BUFFER,
            crate::pane::ROW_HEIGHT,
            layout.cell_height,
            MARGIN,
        )
    }

    /// GridArea.qml canAsk: no window while hidden, while a filter narrows held rows, or while a listing is out.
    fn can_ask(&self, pane: &Pane, shown: Shown) -> bool {
        self.visible && pane.held.total != 0 && !shown.narrowed() && pane.swap.pending.is_none()
    }

    // The pane's one request slot: every ask waits behind what is out, so each reply still meets its own request, and a refused window is not asked again.
    fn ask(pane: &mut Pane, (start, count): (usize, usize)) -> Option<(usize, usize)> {
        let request = WindowRequest {
            start,
            count,
            listing: pane.held.listing,
        };
        if pane.requests.outstanding.is_some() || pane.requests.refused == Some(request) {
            return None;
        }
        pane.requests.outstanding = Some(request);
        pane.requests.sent += 1;
        Some((start, count))
    }

    /// A cursor move: a far landing asks its window first and waits for it; any other move reveals the tile now, in one pass.
    pub fn follow_cursor(
        &mut self,
        pane: &mut Pane,
        shown: Shown,
        focus: &dyn FocusFacts,
    ) -> Option<(usize, usize)> {
        let view = shown.view_of(focus.cursor())?;
        if self.hidden_held {
            return None;
        }
        let layout = self.layout(shown);
        let held = &pane.held;
        let need = geometry::landing_ask(
            &self.plan(&layout),
            view,
            layout.viewport(self.scroll).0,
            held.total,
            held.start,
            held.rows.len(),
        );
        let owes = self.can_ask(pane, shown) && held.row(focus.cursor()).is_none();
        self.landing_wait = need.is_some() && owes;
        self.landing_start = need.map(|need| need.0);
        let asked = need
            .filter(|_| self.can_ask(pane, shown))
            .and_then(|need| Self::ask(pane, need));
        self.counters.landing_asks += asked.is_some() as usize;
        let busy = |need: (usize, usize)| {
            pane.requests
                .outstanding
                .is_some_and(|out| (out.start, out.count) != need)
        };
        self.landing_held = need.filter(|&need| self.landing_wait && asked.is_none() && busy(need));
        if !self.landing_wait {
            self.reveal(&layout, view);
        }
        asked
    }

    fn reveal(&mut self, layout: &Layout, view: usize) {
        if self.visible {
            self.scroll = layout.reveal(self.scroll, view);
            self.counters.reveals += 1;
        }
    }

    /// Call after every window reply or refusal: a held landing asks once the slot is free, and the wait ends on its own reply, its refusal, or once the cursor's rows are held.
    pub fn rows_landed(
        &mut self,
        pane: &mut Pane,
        shown: Shown,
        focus: &dyn FocusFacts,
    ) -> Option<(usize, usize)> {
        if !self.landing_wait {
            return None;
        }
        let owed = self.can_ask(pane, shown) && pane.held.row(focus.cursor()).is_none();
        if let Some(need) = self.landing_held.filter(|_| owed) {
            if pane.requests.outstanding.is_some() {
                return None;
            }
            self.landing_held = None;
            let asked = Self::ask(pane, need);
            self.counters.landing_asks += asked.is_some() as usize;
            if asked.is_some() {
                return asked;
            }
        }
        self.landing_held = None;
        let refused = pane.requests.outstanding.is_none()
            && pane
                .requests
                .refused
                .is_some_and(|out| Some(out.start) == self.landing_start);
        if owed && !refused && Some(pane.held.start) != self.landing_start {
            return None;
        }
        self.end_landing(shown, focus, true);
        None
    }

    /// A refused or failed window reveals the cursor where the rows are; an operator scroll leaves the view where it is.
    pub fn end_landing(&mut self, shown: Shown, focus: &dyn FocusFacts, reveal: bool) {
        self.landing_held = None;
        if !std::mem::take(&mut self.landing_wait) {
            return;
        }
        if let Some(view) = reveal.then(|| shown.view_of(focus.cursor())).flatten() {
            self.reveal(&self.layout(shown), view);
        }
    }

    /// An operator scroll (wheel, drag, flick or bar), which also ends a wait nothing answered.
    pub fn scroll_by(&mut self, shown: Shown, focus: &dyn FocusFacts, delta: f32) {
        self.end_landing(shown, focus, false);
        self.scroll = self.layout(shown).clamp(self.scroll + delta);
    }

    /// GridArea.qml requestIfDrifted: the window the visible tiles owe, never while a landing waits.
    pub fn wanted_window(&mut self, pane: &mut Pane, shown: Shown) -> Option<(usize, usize)> {
        if self.landing_wait || !self.can_ask(pane, shown) {
            return None;
        }
        let layout = self.layout(shown);
        // GridArea.qml asked from contentY, which a notch animates, so the held rows follow what is drawn.
        let (first, last) = layout.viewport(self.drawn_scroll());
        let need = geometry::drift_ask(
            &self.plan(&layout),
            first,
            last,
            pane.held.start,
            pane.held.rows.len(),
            pane.held.total,
        )?;
        let asked = Self::ask(pane, need);
        self.counters.drift_asks += asked.is_some() as usize;
        asked
    }

    /// The listing rows thumbnail and size work may cover: the visible tiles only, none while hidden or listing.
    pub fn wanted_thumbnails(&self, pane: &Pane, shown: Shown) -> Option<RangeInclusive<usize>> {
        if !self.visible || pane.swap.pending.is_some() {
            return None;
        }
        let (first, last) = self.layout(shown).viewport(self.drawn_scroll());
        let last = usize::try_from(last).ok()?;
        shown.span(first.max(0) as usize, last)
    }

    /// Hidden holds nothing and asks nothing; showing again parks the view on the cursor before the first frame.
    pub fn set_visible(&mut self, visible: bool, shown: Shown, focus: &dyn FocusFacts) {
        if visible == self.visible {
            return;
        }
        self.visible = visible;
        if !visible {
            self.landing_wait = false;
            self.landing_held = None;
            self.hidden_held = true;
            self.hovered = None;
            return;
        }
        self.hidden_held = false;
        if let Some(view) = shown.view_of(focus.cursor()) {
            self.reveal(&self.layout(shown), view);
        }
    }

    /// A size, zoom, density or rename height change re-flows the tiles: the first visible tile keeps its place, and a cursor in view stays in view.
    pub fn reflow(&mut self, shown: Shown, focus: &dyn FocusFacts, change: impl FnOnce(&mut Self)) {
        let before = self.layout(shown);
        let cursor = shown
            .view_of(focus.cursor())
            .filter(|&view| before.covers(self.scroll, view));
        let anchor = before.drawn(self.scroll).start;
        let offset = before.row_top(anchor / before.columns) - self.scroll;
        change(self);
        let after = self.layout(shown);
        self.scroll = after.clamp(after.row_top(anchor / after.columns) - offset);
        if let Some(view) = cursor.filter(|_| !self.landing_wait) {
            self.reveal(&after, view);
        }
    }
}
