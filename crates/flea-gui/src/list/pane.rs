//! The list pane's model: the held window of rows, scroll and cursor, and ui/Pane.qml's window policy.
use flea::protocol::{Reply, RowFacts};
use std::rc::Rc;

pub const ROW_HEIGHT: f32 = 31.;
const MIN_BUFFER: usize = 50;
const MAX_BUFFER: usize = 1000;
const DEFAULT_BUFFER: usize = 150;
const REFETCH_MARGIN: usize = 25;

/// One window of one listing as the backend answered it; replaced whole, never edited.
#[derive(Default)]
pub struct Held {
    pub listing: usize,
    pub total: usize,
    pub start: usize,
    pub rows: Vec<RowFacts>,
}

impl Held {
    pub fn row(&self, index: usize) -> Option<&RowFacts> {
        index.checked_sub(self.start).and_then(|at| self.rows.get(at))
    }
}

/// Pane.qml visibleRows: a partial last row counts, and an empty viewport still asks for one.
pub fn visible_rows(height: f32) -> usize {
    ((height / ROW_HEIGHT).ceil() as usize).max(1)
}

/// Pane.qml windowSize: the visible rows plus a buffer either side.
pub fn window_size(visible: usize) -> usize {
    visible + 2 * DEFAULT_BUFFER.clamp(MIN_BUFFER, MAX_BUFFER)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

pub struct Pane {
    path: String,
    pub held: Rc<Held>,
    pub scroll_y: f32,
    pub cursor: usize,
    height: f32,
    pending_total: Option<usize>,
    // The listing request is out, so no window goes until its rows land, Pane.qml listInFlight.
    listing_open: bool,
    // A window asked for and not yet answered, counted as landed when judging drift.
    in_flight: Option<(usize, usize)>,
    // The largest row count any request asked for, the bound an answer is checked against.
    asked: usize,
}

impl Pane {
    pub fn new(path: String, height: f32) -> Self {
        let asked = window_size(visible_rows(height));
        Self {
            path,
            held: Rc::default(),
            scroll_y: 0.,
            cursor: 0,
            height,
            pending_total: None,
            listing_open: true,
            in_flight: None,
            asked,
        }
    }

    /// A listing asked for whose first rows have not landed, Pane.qml's listInFlight.
    pub fn listing_in_flight(&self) -> bool {
        self.listing_open
    }

    /// Unanswered listing and held-window requests.
    pub fn pending(&self) -> usize {
        usize::from(self.listing_open) + usize::from(self.in_flight.is_some())
    }

    /// The listing request's first count, one window as Pane.qml asks for it.
    pub fn first_count(&self) -> usize {
        self.asked
    }

    pub fn visible_rows(&self) -> usize {
        visible_rows(self.height)
    }

    pub fn set_height(&mut self, height: f32) {
        self.height = height;
        self.scroll_by(0.);
    }

    /// Applies one backend reply; true when what the pane draws changed.
    pub fn receive(&mut self, reply: Reply) -> Result<bool, String> {
        match reply {
            Reply::Listed { path, total } if path == self.path => {
                self.pending_total = Some(total);
                Ok(false)
            }
            Reply::Rows { start, rows, listing } => {
                if listing < self.held.listing {
                    return Ok(false);
                }
                let fresh = listing != self.held.listing || self.pending_total.is_some();
                let total = self.pending_total.take().unwrap_or(self.held.total);
                if start > total || rows.len() > total - start || rows.len() > self.asked {
                    return Err("the backend answered rows outside the requested window".into());
                }
                if fresh {
                    self.cursor = 0;
                    self.scroll_y = 0.;
                    self.listing_open = false;
                }
                if self.in_flight.is_some_and(|(asked, _)| asked == start) {
                    self.in_flight = None;
                }
                self.held = Rc::new(Held {
                    listing,
                    total,
                    start,
                    rows,
                });
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn first_visible(&self) -> usize {
        (self.scroll_y / ROW_HEIGHT).floor() as usize
    }

    /// List.qml requestIfDrifted(): a window around the first visible row once a held edge comes within the margin.
    pub fn wanted_window(&mut self) -> Option<(usize, usize)> {
        let total = self.held.total;
        if self.listing_open || total == 0 {
            return None;
        }
        let first = self.first_visible();
        let last = first + self.visible_rows();
        let (held, len) = self
            .in_flight
            .unwrap_or((self.held.start, self.held.rows.len()));
        let held_end = (held + len).min(total);
        let drifted = len == 0
            || (first.saturating_sub(held) < REFETCH_MARGIN && held > 0)
            || (held_end.saturating_sub(last) < REFETCH_MARGIN && held_end < total);
        if !drifted {
            return None;
        }
        let window = (
            first.saturating_sub(DEFAULT_BUFFER.clamp(MIN_BUFFER, MAX_BUFFER)),
            window_size(self.visible_rows()),
        );
        self.in_flight = Some(window);
        self.asked = self.asked.max(window.1);
        Some(window)
    }

    fn max_scroll(&self) -> f32 {
        (self.held.total as f32 * ROW_HEIGHT - self.height).max(0.)
    }

    pub fn scroll_by(&mut self, delta: f32) {
        self.scroll_y = (self.scroll_y + delta).clamp(0., self.max_scroll());
    }

    pub fn scroll_to(&mut self, y: f32) {
        self.scroll_y = y.clamp(0., self.max_scroll());
    }

    /// Moves the cursor and scrolls the least that keeps it whole in view.
    pub fn move_cursor(&mut self, step: Move) -> bool {
        let total = self.held.total;
        if total == 0 {
            return false;
        }
        let page = ((self.height / ROW_HEIGHT).floor() as usize).max(1);
        let last = total - 1;
        self.cursor = match step {
            Move::Up => self.cursor.saturating_sub(1),
            Move::Down => (self.cursor + 1).min(last),
            Move::PageUp => self.cursor.saturating_sub(page),
            Move::PageDown => (self.cursor + page).min(last),
            Move::Home => 0,
            Move::End => last,
        };
        self.reveal_cursor();
        true
    }

    pub fn reveal_cursor(&mut self) {
        let top = self.cursor as f32 * ROW_HEIGHT;
        if top < self.scroll_y {
            self.scroll_to(top);
        } else if top + ROW_HEIGHT > self.scroll_y + self.height {
            self.scroll_to(top + ROW_HEIGHT - self.height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(n: usize) -> Vec<RowFacts> {
        (0..n)
            .map(|i| RowFacts {
                name: format!("{i:06}"),
                ..RowFacts::default()
            })
            .collect()
    }

    fn listed(pane: &mut Pane, total: usize, count: usize) {
        let path = pane.path.clone();
        assert!(!pane.receive(Reply::Listed { path, total }).unwrap());
        assert!(pane
            .receive(Reply::Rows {
                start: 0,
                rows: rows(count),
                listing: 1
            })
            .unwrap());
    }

    #[test]
    fn only_visible_rows_are_counted_including_a_partial_last_row() {
        assert_eq!(visible_rows(0.), 1);
        assert_eq!(visible_rows(31.), 1);
        assert_eq!(visible_rows(62.), 2);
        assert_eq!(visible_rows(63.), 3);
        assert_eq!(visible_rows(640.), 21);
        assert_eq!(window_size(21), 321);
    }

    #[test]
    fn windows_follow_pane_qml_drift_rules_one_request_at_a_time() {
        let mut pane = Pane::new("/files".into(), 620.);
        assert_eq!(pane.wanted_window(), None, "no window while the listing is out");
        listed(&mut pane, 100_000, 320);
        assert_eq!(pane.wanted_window(), None);
        // 320 held rows end at 320; the last visible row reaching within 25 of it asks around the first.
        pane.scroll_to(276. * ROW_HEIGHT);
        assert_eq!(pane.wanted_window(), Some((126, 320)));
        assert_eq!(pane.wanted_window(), None, "the window in flight counts as landed");
        assert!(pane
            .receive(Reply::Rows { start: 126, rows: rows(320), listing: 1 })
            .unwrap());
        assert_eq!(pane.wanted_window(), None);
        pane.scroll_to(140. * ROW_HEIGHT);
        assert_eq!(pane.wanted_window(), Some((0, 320)));
        // A jump past the held window asks around the new first row.
        pane.in_flight = None;
        pane.scroll_to(50_000. * ROW_HEIGHT);
        assert_eq!(pane.wanted_window(), Some((49_850, 320)));
    }

    #[test]
    fn stale_and_oversized_rows_are_refused_and_the_cursor_stays_in_view() {
        let mut pane = Pane::new("/files".into(), 620.);
        listed(&mut pane, 1000, 320);
        assert!(pane
            .receive(Reply::Rows { start: 0, rows: rows(321), listing: 1 })
            .is_err());
        assert!(pane
            .receive(Reply::Rows { start: 990, rows: rows(11), listing: 1 })
            .is_err());
        assert!(!pane
            .receive(Reply::Rows { start: 0, rows: rows(3), listing: 0 })
            .unwrap());
        assert!(pane.move_cursor(Move::End));
        assert_eq!(pane.cursor, 999);
        assert_eq!(pane.scroll_y, 1000. * ROW_HEIGHT - 620.);
        pane.move_cursor(Move::PageUp);
        assert_eq!(pane.cursor, 979);
        pane.move_cursor(Move::Home);
        assert_eq!((pane.cursor, pane.scroll_y), (0, 0.));
    }
}
