//! What each drawn track shows, rebuilt once per event so a still frame reads it without work.
use super::cache::Row;
use super::geometry::{ancestor_shown, contain_y, Layout, Slot};
use super::landing::Landing;
use super::peeks::{Lens, PeekRow, Peeks};
use super::preview_host::{Host, PreviewSettings};
use crate::list::pane::{Held, ROW_HEIGHT};
use crate::navigation::history::{join, leaf_of, parent_of};
use std::rc::Rc;

/// Where a track's rows come from: a peek, the pane's own listing, or the file preview the producer draws.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackRows {
    Peek(Rc<[PeekRow]>),
    Listing,
    Preview,
    Blank,
}

/// The tile a track draws over its rows, which the hero producer paints in the track's rect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackState {
    Rows,
    Loading,
    Empty,
    Locked(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackView {
    pub slot: Slot,
    pub x: f32,
    pub width: f32,
    /// The directory this track draws, "" for a blank slot or the preview.
    pub path: String,
    pub rows: TrackRows,
    /// Ancestors draw muted names; the active track and the child draw the foreground.
    pub dim: bool,
    /// The name of the directory the next track draws, lifted in this one.
    pub lifted: Option<String>,
    /// The lifted row's index among the rows drawn, which the track keeps whole on screen.
    pub lifted_at: Option<usize>,
    pub divider: bool,
    pub state: TrackState,
    /// False while a stand-in from the last listing draws, which takes no input.
    pub enabled: bool,
}

impl TrackView {
    /// The 1 px divider at the track's right edge, None when the track draws none.
    pub fn divider_x(&self) -> Option<f32> {
        self.divider
            .then_some(self.x + self.width - super::geometry::DIVIDER)
    }
}

pub(super) struct Shown<'a> {
    pub host: &'a Host,
    pub settings: PreviewSettings,
}

pub(super) struct Tracks {
    views: Rc<[TrackView]>,
    pub height: f32,
    /// Row density's row, which every peek track draws at.
    row: f32,
    pub constructed: usize,
    pub dropped: usize,
    /// Each peek slot's scroll and the path and lifted row it was last contained for.
    scrolls: [(String, Option<usize>, f32); 5],
}

impl Tracks {
    pub fn new(height: f32) -> Self {
        Self {
            views: Rc::from([]),
            height,
            row: ROW_HEIGHT,
            constructed: 0,
            dropped: 0,
            scrolls: Default::default(),
        }
    }

    pub fn views(&self) -> &Rc<[TrackView]> {
        &self.views
    }

    /// A peeked track's rows and tile: the landing's stand-in first, then the answered rows, a denial or the empty tile.
    /// `stands`: the landing may hold a stand-in for this path, an ancestor's or the child's at entry.
    fn peek(
        path: &str,
        shown: bool,
        lens: &Lens<'_>,
        peeks: &Peeks,
        landing: &Landing,
        stands: bool,
    ) -> (TrackRows, TrackState, bool) {
        let here = lens.here;
        let key = lens.key(path);
        let held = peeks.rows(&key).cloned().unwrap_or_else(|| Rc::from([]));
        let rows = if shown {
            landing.rows(path, here, held)
        } else {
            Rc::from([])
        };
        let waiting = landing.waiting(path, here);
        let loading = stands && landing.loading(path, here);
        let mode = if shown {
            landing.mode(path, here, peeks.denied(&key))
        } else {
            None
        };
        let empty = shown && peeks.answered(&key, path) && (!stands || !waiting);
        let state = match mode {
            _ if loading => TrackState::Loading,
            Some(mode) => TrackState::Locked(mode),
            None if empty && rows.is_empty() => TrackState::Empty,
            None => TrackState::Rows,
        };
        let rows = if shown {
            TrackRows::Peek(rows)
        } else {
            TrackRows::Blank
        };
        (rows, state, !stands || !waiting)
    }

    /// Rebuild every drawn track without publishing a loading mark before the renderer draws it.
    pub fn rebuild(
        &mut self,
        layout: &Layout,
        lens: Lens<'_>,
        peeks: &Peeks,
        landing: &Landing,
        shown: Shown<'_>,
    ) {
        let here = lens.here;
        let mut views = Vec::with_capacity(layout.count);
        for track in layout.tracks() {
            let depth = track.slot.depth();
            let view = match track.slot {
                Slot::Active => TrackView {
                    slot: track.slot,
                    x: track.x,
                    width: track.width,
                    path: here.to_owned(),
                    rows: TrackRows::Listing,
                    dim: false,
                    lifted: None,
                    lifted_at: None,
                    divider: shown.host.third_shown(shown.settings) || shown.host.shown.has_row,
                    state: TrackState::Rows,
                    enabled: true,
                },
                Slot::Child if shown.host.shown.is_dir => {
                    let path = shown.host.shown.child.clone();
                    let (rows, state, enabled) =
                        Self::peek(&path, !path.is_empty(), &lens, peeks, landing, true);
                    let enabled = enabled
                        && shown.host.fallback_at.is_none()
                        && peeks.answered(&lens.key(&path), &path);
                    TrackView {
                        slot: track.slot,
                        x: track.x,
                        width: track.width,
                        path,
                        rows,
                        dim: false,
                        lifted: None,
                        lifted_at: None,
                        divider: false,
                        state,
                        enabled,
                    }
                }
                Slot::Child => TrackView {
                    slot: track.slot,
                    x: track.x,
                    width: track.width,
                    path: String::new(),
                    rows: if shown.host.preview_visible(shown.settings) {
                        TrackRows::Preview
                    } else {
                        TrackRows::Blank
                    },
                    dim: false,
                    lifted: None,
                    lifted_at: None,
                    divider: false,
                    state: TrackState::Rows,
                    enabled: true,
                },
                _ => {
                    let mut next = here;
                    for _ in 1..depth {
                        next = parent_of(next);
                    }
                    let path = parent_of(next).to_owned();
                    let drawn = ancestor_shown(here, depth);
                    let (rows, state, enabled) =
                        Self::peek(&path, drawn, &lens, peeks, landing, true);
                    let lifted = drawn.then(|| leaf_of(next).to_owned());
                    let lifted_at = match (&rows, &lifted) {
                        (TrackRows::Peek(rows), Some(name)) => {
                            rows.iter().position(|row| row.name == *name)
                        }
                        _ => None,
                    };
                    TrackView {
                        slot: track.slot,
                        x: track.x,
                        width: track.width,
                        path,
                        rows,
                        dim: true,
                        lifted,
                        lifted_at,
                        divider: true,
                        state,
                        enabled,
                    }
                }
            };
            views.push(view);
        }
        let before = self.views.len();
        let after = views.len();
        self.constructed += after.saturating_sub(before);
        self.dropped += before.saturating_sub(after);
        for view in views.iter().filter(|view| view.slot != Slot::Active) {
            self.contain(view);
        }
        if *self.views != *views {
            self.views = Rc::from(views);
        }
    }

    /// A share or width change over the same slots moves each drawn track in place, its rows untouched.
    pub fn place(&mut self, layout: &Layout) {
        let same = layout.tracks().count() == self.views.len()
            && layout
                .tracks()
                .zip(self.views.iter())
                .all(|(track, view)| track.slot == view.slot);
        let moved = layout
            .tracks()
            .zip(self.views.iter())
            .any(|(track, view)| track.x != view.x || track.width != view.width);
        if same && moved {
            self.views = layout
                .tracks()
                .zip(self.views.iter())
                .map(|(track, view)| TrackView {
                    x: track.x,
                    width: track.width,
                    ..view.clone()
                })
                .collect();
        }
    }

    /// A new directory starts at the top, and a moved lifted row scrolls just enough to show whole, in the same pass.
    fn contain(&mut self, view: &TrackView) {
        let total = match &view.rows {
            TrackRows::Peek(rows) => rows.len(),
            _ => 0,
        };
        let held = &mut self.scrolls[view.slot.index()];
        if held.0 != view.path {
            *held = (view.path.clone(), None, 0.);
        }
        if held.1 != view.lifted_at {
            held.1 = view.lifted_at;
            if let Some(at) = view.lifted_at {
                held.2 = contain_y(at, total, held.2, self.height, self.row);
            }
        }
    }

    pub fn scroll(&self, slot: Slot) -> f32 {
        self.scrolls[slot.index()].2
    }

    /// A wheel or drag over a peek track, clamped to its rows and ColumnPane.qml's footer band below them.
    pub fn scroll_by(&mut self, slot: Slot, dy: f32) -> f32 {
        let total = self
            .views
            .iter()
            .find(|view| view.slot == slot)
            .map_or(0, |view| match &view.rows {
                TrackRows::Peek(rows) => rows.len(),
                _ => 0,
            });
        let end = (total as f32 * self.row + super::geometry::FOOTER - self.height).max(0.);
        let held = &mut self.scrolls[slot.index()];
        held.2 = (held.2 + dy).clamp(0., end);
        held.2
    }

    /// A density change keeps each peek track's lifted row whole and its scroll inside its rows.
    fn set_row(&mut self, row: f32) {
        if self.row == row {
            return;
        }
        self.row = row;
        let views = Rc::clone(&self.views);
        for view in views.iter().filter(|view| view.slot != Slot::Active) {
            self.scrolls[view.slot.index()].1 = None;
            self.contain(view);
            self.scroll_by(view.slot, 0.);
        }
    }
}

/// The peek tracks' own scroll; the active track's is the pane's.
impl super::Area {
    /// askDropDevs: a drag entering the pane asks each drawn peek track's visible folder rows, in slot order.
    pub fn drop_asks(&self) -> Vec<String> {
        if !self.visible {
            return Vec::new();
        }
        let pitch = self.tracks.row;
        let shown = (self.tracks.height / pitch).ceil().max(1.);
        let mut asks = Vec::new();
        for view in self
            .tracks()
            .iter()
            .filter(|view| view.slot != Slot::Active)
        {
            let TrackRows::Peek(rows) = &view.rows else {
                continue;
            };
            let scroll = self.scroll(view.slot);
            let first = (scroll / pitch).floor() as usize;
            let end = ((scroll / pitch) + shown).ceil() as usize;
            let folders = rows.iter().take(end).skip(first).filter(|row| row.dir);
            asks.extend(folders.map(|row| join(&view.path, &row.name)));
        }
        asks
    }

    pub fn scroll(&self, slot: Slot) -> f32 {
        self.tracks.scroll(slot)
    }

    pub fn scroll_by(&mut self, slot: Slot, dy: f32) -> f32 {
        self.tracks.scroll_by(slot, dy)
    }

    /// Row density's row, the pitch every peek track scrolls and asks by.
    pub fn row(&self) -> f32 {
        self.tracks.row
    }

    pub fn set_row(&mut self, row: f32) {
        self.tracks.set_row(row);
    }

    #[cfg(test)]
    pub fn height(&self) -> f32 {
        self.tracks.height
    }
}

pub(crate) fn row_at<'a>(
    rows: &'a TrackRows,
    held: Option<&'a Held>,
    index: usize,
) -> Option<Row<'a>> {
    match rows {
        TrackRows::Peek(rows) => rows.get(index).map(Row::Peek),
        TrackRows::Listing => held?.row(index).map(Row::Listing),
        TrackRows::Preview | TrackRows::Blank => None,
    }
}

/// The row a track lifts, the one named on the path to the active track; the capture probe reads the same rule.
pub(crate) fn lifted_row(view: &TrackView, held: Option<&Held>, index: usize) -> bool {
    view.lifted
        .as_deref()
        .is_some_and(|name| row_at(&view.rows, held, index).is_some_and(|row| row.name() == name))
}
