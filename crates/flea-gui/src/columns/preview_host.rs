//! The third column's host: what it shows as of the last swap, when a cursor move lands, and the preview edge's drag.
use super::geometry::{folder_data_hold, Layout};
use crate::pane::swap::CAP_MS;

pub use crate::pane::swap::HOLD_MS;
/// Qt's platform startDragDistance: a press moved this far or less is a click, never a drag.
pub const START_DRAG_DISTANCE: f32 = 10.;
/// ResizeHandle's zone width, centred on the hairline just left of the edge.
pub const EDGE_ZONE: f32 = 9.;
pub const EDGE_MARK: f32 = 16.;

/// The cursor row's shape as the third column reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor<'a> {
    pub row: bool,
    pub is_dir: bool,
    pub child: &'a str,
}

/// What the third column shows: the cursor row as of the last swap, so a picture is taken before it changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub has_row: bool,
    pub is_dir: bool,
    pub child: String,
}

impl Shown {
    fn matches(&self, cursor: Cursor<'_>) -> bool {
        cursor.is_dir == self.is_dir && cursor.child == self.child && cursor.row == self.has_row
    }
}

/// Settings View's preview switches and the storage class's manual hold, which decide whether a file row is a load.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PreviewSettings {
    pub column: bool,
    pub automatic: bool,
    pub manual_hold: bool,
}

/// The third column's swap, ui/PreviewSwap.qml: a hold keeps the last frame until ready or the cap; G2 previews owns it.
pub trait ThirdSwap {
    /// Neither capturing nor holding.
    fn idle(&self) -> bool;
    /// Hold the last frame under this key; the owner calls Host::show_cursor_row when the hold releases.
    fn hold(&mut self, key: &str);
    fn start(&mut self, file_load: bool);
    fn stop_cap(&mut self);
    fn cancel(&mut self);
}

/// The file preview the third column hosts, G2 previews' SelectionPreview; no decoder, player or renderer lives here.
pub trait PreviewProducer {
    fn ready(&self) -> bool;
    fn follow_selection(&mut self);
    fn arm_settle(&mut self);
    fn ask_cursor_thumb(&mut self);
    /// One flea.column* preview function forwarded whole; None while the producer has not ported it.
    fn ipc(&self, function: &str, arguments: &[String]) -> Option<String>;
}

/// The moment a cursor move reads besides the cursor itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Moment {
    /// A carried landing in flight with no cursor row yet: the rows empty before they fill.
    pub carried_gap: bool,
    /// A filter hid the cursor row with rows still shown; the move lands one event later.
    pub filter_hid_cursor: bool,
    pub settled: bool,
    pub answered_child: bool,
    pub file_row: bool,
}

/// What one moveThird pass decided, for a test and for the host's caller to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Same,
    KeptThroughCarry,
    SettleLater,
    FolderWait,
    JoinedHold,
    Landed,
    HeldFile,
}

#[derive(Debug, Default)]
pub struct Host {
    pub shown: Shown,
    /// The folder fallback's deadline, HOLD_MS after an unanswered folder starts its wait.
    pub fallback_at: Option<u64>,
    pub edge: Edge,
}

impl Host {
    /// showCursorRow: a stale queued show never draws an unanswered folder; the fallback's own show passes force.
    pub fn show_cursor_row(
        &mut self,
        cursor: Cursor<'_>,
        answered_child: bool,
        force: bool,
    ) -> bool {
        if !force && folder_data_hold(cursor.is_dir, answered_child) {
            return false;
        }
        self.shown = Shown {
            has_row: cursor.row,
            is_dir: cursor.is_dir,
            child: cursor.child.to_owned(),
        };
        true
    }

    /// moveThird: only a file load takes a hold and an unanswered folder waits by data.
    pub fn move_third(
        &mut self,
        cursor: Cursor<'_>,
        moment: Moment,
        settings: PreviewSettings,
        ports: &mut super::Ports<'_>,
        key: &str,
        now: u64,
    ) -> Move {
        let super::Ports { swap, preview } = ports;
        if self.shown.matches(cursor) {
            return Move::Same;
        }
        if moment.carried_gap {
            return Move::KeptThroughCarry;
        }
        if !moment.settled && moment.filter_hid_cursor {
            return Move::SettleLater;
        }
        let idle = swap.idle();
        let file_load =
            moment.file_row && settings.column && settings.automatic && !settings.manual_hold;
        if folder_data_hold(cursor.is_dir, moment.answered_child) {
            swap.stop_cap();
            self.fallback_at = Some(now + HOLD_MS);
            return Move::FolderWait;
        }
        self.fallback_at = None;
        if !file_load {
            let moved = if idle {
                self.show_cursor_row(cursor, moment.answered_child, false);
                Move::Landed
            } else {
                // A hold already live owns the picture, so the no-load change joins it under the cap.
                swap.hold(key);
                swap.start(false);
                Move::JoinedHold
            };
            if !cursor.is_dir {
                preview.follow_selection();
            }
            return moved;
        }
        swap.hold(key);
        preview.arm_settle();
        Move::HeldFile
    }

    /// The fallback's deadline: a folder still unanswered shows its pending state and the live picture ends first.
    pub fn fall_back(
        &mut self,
        cursor: Cursor<'_>,
        answered_child: bool,
        swap: &mut dyn ThirdSwap,
        now: u64,
    ) -> bool {
        if self.fallback_at.is_none_or(|at| at > now) {
            return false;
        }
        self.fallback_at = None;
        if !cursor.is_dir || answered_child {
            return false;
        }
        swap.cancel();
        self.show_cursor_row(cursor, answered_child, true)
    }

    /// A landed peek shows the waiting folder whole; a live picture ends before those rows do.
    pub fn peek_landed(
        &mut self,
        cursor: Cursor<'_>,
        answered_child: bool,
        swap: &mut dyn ThirdSwap,
    ) -> bool {
        let shown = answered_child && !cursor.child.is_empty() && cursor.child != self.shown.child;
        if shown {
            swap.cancel();
            self.show_cursor_row(cursor, answered_child, false);
        }
        if answered_child {
            self.fallback_at = None;
        }
        shown
    }

    /// The swap's answer for the third column: the folder's rows in, or the file's preview whole.
    pub fn ready(
        &self,
        answered_shown: bool,
        settings: PreviewSettings,
        preview_ready: bool,
    ) -> bool {
        if self.shown.is_dir {
            answered_shown
        } else {
            !self.shown.has_row || !settings.column || preview_ready
        }
    }

    /// The preview column counts as shown when the child folder or the file preview draws, else the active track is rightmost.
    pub fn third_shown(&self, settings: PreviewSettings) -> bool {
        self.shown.is_dir || (self.shown.has_row && settings.column)
    }

    /// The file preview draws in the third column, never for a folder.
    pub fn preview_visible(&self, settings: PreviewSettings) -> bool {
        self.shown.has_row && !self.shown.is_dir && settings.column
    }
}

/// The swap's key: the listing path and the cursor index.
pub fn swap_key(path: &str, cursor: usize) -> String {
    format!("{path}\n{cursor}")
}

/// The cap a folder data hold never takes: an unanswered folder waits on its reply or HOLD_MS, never CAP_MS.
pub const FOLDER_WAIT_EXCEEDS_CAP: bool = HOLD_MS > CAP_MS;

/// PreviewEdge: a press past the drag distance drags the third column's width live and stores its share once.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edge {
    pub dragging: bool,
    /// The pointer is over the zone, which shows the line and mark at rest.
    pub hovered: bool,
    pulled: bool,
    press_x: f32,
    from: f32,
    /// dragLast: the live third width while a pulled drag holds it.
    pub last: Option<f32>,
}

/// What the edge asks Settings to store: a share of 1 to 100, or 0 for the default third.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Store(pub u32);

impl Edge {
    /// The zone's left and width, centred on the hairline just left of the third column.
    pub fn zone(layout: &Layout) -> (f32, f32) {
        (layout.edge() - 1. - (EDGE_ZONE / 2.).floor(), EDGE_ZONE)
    }

    /// ResizeHandle's accent line, 1 px on the hairline left of the third column, while hovered or dragged.
    pub fn line(&self, layout: &Layout, height: f32) -> Option<(f32, f32, f32, f32)> {
        (self.hovered || self.dragging).then(|| (layout.edge() - 1., 0., 1., height))
    }

    /// The move-horizontal mark, 16 px square, centred on the hairline with its top at mid height.
    pub fn mark(&self, layout: &Layout, height: f32) -> Option<(f32, f32, f32)> {
        (self.hovered || self.dragging)
            .then(|| (layout.edge() - 0.5 - EDGE_MARK / 2., height / 2., EDGE_MARK))
    }

    pub fn press(&mut self, x: f32, layout: &Layout) {
        self.press_x = x;
        self.dragging = true;
        self.pulled = false;
        self.from = layout.third_width;
    }

    /// A held move: nothing until the pointer passes the drag distance, then the clamped width follows it.
    pub fn moved(&mut self, x: f32, layout: &Layout) -> bool {
        if !self.dragging {
            return false;
        }
        let shift = (x - self.press_x).round();
        if !self.pulled && shift.abs() > START_DRAG_DISTANCE {
            self.pulled = true;
        }
        if !self.pulled {
            return false;
        }
        let last = layout.clamp_last(self.from - shift);
        let changed = self.last != Some(last);
        self.last = Some(last);
        changed
    }

    /// Release, a lost grab and the edge hidden mid-drag all end the drag, storing the width the user saw.
    pub fn release(&mut self, width: f32) -> Option<Store> {
        self.dragging = false;
        self.last
            .take()
            .filter(|_| width > 0.)
            .map(|last| Store(((100. * last / width).round() as u32).clamp(1, 100)))
    }

    /// A double click restores the default third and stores 0.
    pub fn restore(&mut self) -> Store {
        self.dragging = false;
        self.last = None;
        Store(0)
    }
}

/// The preview edge as the area drives it: every drag step re-lays the tracks, and a release stores the share once.
impl super::Area {
    /// The edge's stored share: a drag's release writes its percent once, a double click 0.
    pub fn store_edge(&mut self, store: Option<Store>) -> Option<u32> {
        let store = store?;
        self.share = f64::from(store.0);
        self.stored.push(store.0);
        self.relayout();
        Some(store.0)
    }

    /// The pointer moved over the area: the zone is hot only while the third column shows.
    pub fn edge_hover(&mut self, x: f32) {
        let (left, width) = Edge::zone(&self.layout);
        self.host.edge.hovered =
            self.host.third_shown(self.settings) && (left..left + width).contains(&x);
    }

    pub fn edge_press(&mut self, x: f32) {
        self.host.edge.press(x, &self.layout);
    }

    pub fn edge_move(&mut self, x: f32) -> bool {
        let moved = self.host.edge.moved(x, &self.layout);
        if moved {
            self.relayout();
        }
        moved
    }

    /// Release and a lost grab both end the drag, storing the width the user saw.
    pub fn edge_release(&mut self) -> Option<u32> {
        let store = self.host.edge.release(self.layout.width);
        let stored = self.store_edge(store);
        self.relayout();
        stored
    }

    pub fn edge_restore(&mut self) -> u32 {
        let store = self.host.edge.restore();
        self.store_edge(Some(store)).unwrap_or(0)
    }
}
