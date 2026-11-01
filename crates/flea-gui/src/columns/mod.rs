//! The Columns view: one track per drawn column over the pane's listing and its peeked neighbours, the third hosting the cursor row.
pub mod cache;
pub mod carry;
pub mod edge;
pub mod element;
pub mod geometry;
mod glyphs;
pub mod landing;
pub mod menu;
pub mod paint;
pub mod peeks;
pub mod preview_host;
mod resize;
pub mod sets;
mod tracks;
pub(crate) mod walks;

/// The listing window and face the active track reads, re-exported read-only for the shell and the unit tests.
pub use crate::list::{pane::Held, Typeface};
use crate::navigation::history::join;
/// The elision every track's names take, for a reader that checks a drawn name.
pub use crate::row::middle_elide;
use carry::Plan;
use flea::protocol::RowFacts;
use geometry::{keep_asks, neighbour_asks, Layout};
use landing::Landing;
use peeks::{Landed, Lens, Peeked, Peeks};
use preview_host::{
    swap_key, Cursor, Host, Moment, Move, PreviewProducer, PreviewSettings, ThirdSwap,
};
use std::rc::Rc;
pub use tracks::{TrackRows, TrackState, TrackView};

/// The pane facts a Columns event reads, a snapshot the shell builds from the committed pane.
#[derive(Clone, Copy)]
pub struct PaneView<'a> {
    pub lens: Lens<'a>,
    /// Search.baseOf: a search's base, else the listing's own path.
    pub base: &'a str,
    /// The cursor's row while the filter shows it, else None.
    pub cursor_row: Option<&'a RowFacts>,
    pub cursor: usize,
    /// The held window's listing generation, so rows landing under an unmoved cursor still follow it.
    pub listing: usize,
    pub in_flight: bool,
    /// The filter hid the cursor row with rows still shown.
    pub filter_hid_cursor: bool,
}

impl PaneView<'_> {
    pub(crate) fn child(&self) -> String {
        self.cursor_row
            .filter(|row| row.dir)
            .map(|row| join(self.base, &row.name))
            .unwrap_or_default()
    }
}

/// The two producers the third column hosts, owned by G2 previews.
pub struct Ports<'a> {
    pub swap: &'a mut dyn ThirdSwap,
    pub preview: &'a mut dyn PreviewProducer,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Seen {
    visible: bool,
    count: usize,
    order: (bool, bool),
    child: String,
    cursor: (usize, usize),
    shown_child: String,
}

/// ColumnsArea: the peek store, the landing, the third column's host and the drawn keep set, driven by pane events.
pub struct Area {
    pub layout: Layout,
    pub visible: bool,
    pub limit: Option<f64>,
    pub share: f64,
    pub settings: PreviewSettings,
    pub peeks: Peeks,
    pub landing: Landing,
    pub host: Host,
    pub carry: Option<Plan>,
    /// keep.drawn: every column directory drawn at the last refresh, which each watching peek names.
    pub drawn: Vec<String>,
    /// Peek lines for the backend, drained by the shell's transport.
    pub outbox: Vec<String>,
    /// Every previewWidth share the edge stored, in order, drained by the shell into View settings.
    pub stored: Vec<u32>,
    /// preview.tracks as weights, empty for equal columns left of the third.
    pub weights: Vec<f32>,
    /// The weights a column hairline stored last, drained by the shell into View settings.
    pub stored_tracks: Option<Vec<u32>>,
    pub peek_requests: usize,
    /// The child path whose empty reply landed settled, so its hero skips the entrance.
    pub hero_settled: Option<String>,
    /// Qt.callLater's settle: the time it was deferred, due at once on the next tick.
    settle_later: Option<u64>,
    seen: Seen,
    tracks: tracks::Tracks,
}

impl Area {
    pub fn new(
        width: f32,
        height: f32,
        limit: Option<f64>,
        share: f64,
        settings: PreviewSettings,
    ) -> Self {
        Self {
            layout: Layout::new(width, limit, share, None),
            visible: true,
            limit,
            share,
            settings,
            peeks: Peeks::default(),
            landing: Landing::default(),
            host: Host::default(),
            carry: None,
            drawn: Vec::new(),
            outbox: Vec::new(),
            stored: Vec::new(),
            weights: Vec::new(),
            stored_tracks: None,
            peek_requests: 0,
            hero_settled: None,
            settle_later: None,
            seen: Seen::default(),
            tracks: tracks::Tracks::new(height),
        }
    }

    pub fn count(&self) -> usize {
        self.layout.count
    }

    /// The drawn tracks as of the last event, which the Elements paint without rebuilding.
    pub fn tracks(&self) -> &Rc<[TrackView]> {
        self.tracks.views()
    }

    pub fn tracks_built(&self) -> usize {
        self.tracks.views().len()
    }

    pub fn tracks_constructed(&self) -> usize {
        self.tracks.constructed
    }

    pub fn tracks_dropped(&self) -> usize {
        self.tracks.dropped
    }

    fn relayout(&mut self) {
        let drag = self.host.edge.last;
        let weights = self.host.edge.widths.as_deref().unwrap_or(&self.weights);
        self.layout = Layout::weighted(self.layout.width, self.limit, self.share, drag, weights);
        self.tracks.place(&self.layout);
    }

    /// A resize re-lays every track in the same pass, with no re-read; a changed count draws at the next sync, which asks the new set.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.layout.width = width;
        self.tracks.height = height;
        self.relayout();
    }

    pub fn set_limit(&mut self, limit: Option<f64>) {
        self.limit = limit;
        self.relayout();
    }

    fn cursor<'a>(pane: &PaneView<'_>, child: &'a str) -> Cursor<'a> {
        Cursor {
            row: pane.cursor_row.is_some(),
            is_dir: pane.cursor_row.is_some_and(|row| row.dir),
            child,
        }
    }

    fn answered(&self, lens: &Lens<'_>, path: &str) -> bool {
        self.peeks.answered(&lens.key(path), path)
    }

    /// ask(): again re-asks a held column, rearm asks with the first ask out, because the backend unwatched it.
    pub fn ask(&mut self, lens: &Lens<'_>, path: &str, again: bool, rearm: bool) {
        let ask = lens.ask(path, &self.drawn);
        if self.peeks.should_ask(&ask, again, rearm) {
            self.peeks.pending.track(&ask.sent());
            self.outbox.push(ask.line());
            self.peek_requests += 1;
        }
    }

    /// refreshNeighbours: a hidden view asks nothing, nor a carried navigation before its rows land.
    pub fn refresh(&mut self, pane: &PaneView<'_>, ports: &mut Ports<'_>) {
        if !self.visible || self.carry.is_some() && pane.in_flight {
            return;
        }
        let lens = pane.lens;
        let asks = neighbour_asks(lens.here, self.layout.width, self.limit);
        let plan = keep_asks(&mut self.drawn, &asks, &pane.child());
        let shown = &self.host.shown;
        if shown.is_dir && !shown.child.is_empty() && !self.drawn.contains(&shown.child) {
            self.drawn.push(shown.child.clone());
        }
        for (path, again) in plan {
            self.ask(&lens, &path, again, again);
        }
        ports.preview.follow_selection();
    }

    /// moveThird for the cursor as the pane holds it now.
    pub fn move_third(
        &mut self,
        pane: &PaneView<'_>,
        ports: &mut Ports<'_>,
        settled: bool,
        now: u64,
    ) -> Move {
        let child = pane.child();
        let moment = Moment {
            carried_gap: self.carry.is_some() && pane.in_flight && pane.cursor_row.is_none(),
            filter_hid_cursor: pane.filter_hid_cursor,
            settled,
            answered_child: self.answered(&pane.lens, &child),
            file_row: pane.cursor_row.is_some_and(|row| !row.dir),
        };
        let key = swap_key(pane.lens.here, pane.cursor);
        let moved = self.host.move_third(
            Self::cursor(pane, &child),
            moment,
            self.settings,
            ports,
            &key,
            now,
        );
        self.settle_later = (moved == Move::SettleLater).then_some(now);
        moved
    }

    /// The one entry after any pane change: each derived value that moved runs its ColumnsArea handler, in order.
    pub fn sync(&mut self, pane: &PaneView<'_>, ports: &mut Ports<'_>, now: u64) {
        if self.settle_later.take().is_some() && pane.filter_hid_cursor {
            self.move_third(pane, ports, true, now);
        }
        let child = pane.child();
        // A settled entrance lasts its own landing: the hero entrance runs again once the child leaves.
        if self
            .hero_settled
            .as_deref()
            .is_some_and(|path| path != child)
        {
            self.hero_settled = None;
        }
        let seen = Seen {
            visible: self.visible,
            count: self.layout.count,
            order: (pane.lens.hidden, pane.lens.hidden_last),
            child: child.clone(),
            cursor: (pane.cursor, pane.listing),
            shown_child: self.seen.shown_child.clone(),
        };
        let first = self.seen == Seen::default() && self.tracks.views().is_empty();
        if first {
            let cursor = Self::cursor(pane, &child);
            self.host
                .show_cursor_row(cursor, self.answered(&pane.lens, &child), false);
            self.refresh(pane, ports);
        } else if seen.visible != self.seen.visible
            || seen.count != self.seen.count
            || seen.order != self.seen.order
        {
            self.refresh(pane, ports);
        }
        if !first && seen.child != self.seen.child {
            ports.preview.ask_cursor_thumb();
            self.refresh(pane, ports);
            self.move_third(pane, ports, false, now);
        } else if !first && seen.cursor != self.seen.cursor {
            ports.preview.ask_cursor_thumb();
            self.move_third(pane, ports, false, now);
            ports.preview.follow_selection();
        }
        if pane.cursor_row.is_some() != self.host.shown.has_row {
            self.move_third(pane, ports, false, now);
        }
        // The edge hidden under a held button ends the drag like a release; a column hairline stays.
        if !self.host.third_shown(self.settings) && self.host.edge.at == geometry::Line::Preview {
            self.host.edge.hovered = false;
            if self.host.edge.dragging {
                self.edge_release();
            }
        }
        self.seen = Seen {
            shown_child: self.host.shown.child.clone(),
            ..seen
        };
        if self.seen.shown_child != seen.shown_child {
            self.refresh(pane, ports);
        }
        self.rebuild(pane);
    }

    /// onPeeked: a reply this view asked for under the folder's current order lands, the rest drop.
    pub fn peeked(
        &mut self,
        reply: &Peeked,
        pane: &PaneView<'_>,
        ports: &mut Ports<'_>,
        now: u64,
    ) -> Landed {
        let lens = pane.lens;
        let child = pane.child();
        let cursor_is_dir = pane.cursor_row.is_some_and(|row| row.dir);
        let landed = self.peeks.land(reply, &lens.tag(&reply.path));
        if landed != Landed::Stored {
            return landed;
        }
        let settles = geometry::should_settle_hero(
            cursor_is_dir,
            &reply.path,
            &child,
            reply.denied.is_some(),
            reply.rows.len(),
        );
        if settles {
            self.hero_settled = Some(reply.path.clone());
        }
        self.landing.answered(&reply.path);
        let answered = self.answered(&lens, &child);
        self.host
            .peek_landed(Self::cursor(pane, &child), answered, ports.swap);
        self.sync(pane, ports, now);
        landed
    }

    /// The backend says a watched column directory changed: a drawn, answered column asks again.
    pub fn changed(&mut self, path: &str, lens: &Lens<'_>) {
        if self.drawn.iter().any(|drawn| drawn == path) && self.answered(lens, path) {
            self.ask(lens, path, true, false);
        }
    }

    /// A folder's remembered order changed with the path unchanged: each drawn column held under another order asks again.
    pub fn orders_changed(&mut self, lens: &Lens<'_>) {
        if self.visible {
            for path in self.drawn.clone() {
                self.ask(lens, &path, false, false);
            }
        }
    }

    /// The view shown or hidden; becoming visible asks for everything, since nothing else changed.
    pub fn set_visible(
        &mut self,
        visible: bool,
        pane: &PaneView<'_>,
        ports: &mut Ports<'_>,
        now: u64,
    ) {
        self.visible = visible;
        if !visible {
            self.host.edge.hovered = false;
            if self.host.edge.dragging {
                self.edge_release();
            }
        }
        self.sync(pane, ports, now);
    }

    /// One timer serves the landing's cap and minimum and the folder fallback.
    pub fn tick(&mut self, pane: &PaneView<'_>, ports: &mut Ports<'_>, now: u64) {
        self.landing.tick(now);
        let child = pane.child();
        let answered = self.answered(&pane.lens, &child);
        self.host
            .fall_back(Self::cursor(pane, &child), answered, ports.swap, now);
        self.sync(pane, ports, now);
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.landing
            .next_deadline()
            .into_iter()
            .chain(self.host.fallback_at)
            .chain(self.settle_later)
            .min()
    }

    fn rebuild(&mut self, pane: &PaneView<'_>) {
        let shown = tracks::Shown {
            host: &self.host,
            settings: self.settings,
        };
        self.tracks
            .rebuild(&self.layout, pane.lens, &self.peeks, &self.landing, shown);
    }
}
