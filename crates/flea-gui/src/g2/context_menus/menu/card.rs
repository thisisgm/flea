//! Retained card geometry, scrolling and row hit testing.
use super::{rows, Built, Entry, Rect};

pub struct Card {
    pub entries: Vec<Entry>,
    pub built: Built,
    pub frame: Rect,
    pub content_y: i32,
    pub view_h: i32,
    // The view the work area allows before a half-row trim.
    pub(super) full_h: i32,
    pub cursor: Option<usize>,
    // The wheel's and the touchpad's leftover travel, one each per card, as each v0.3.12 CardScroll keeps its own.
    pub(super) notch: i32,
    pub(super) touch: f32,
    // The busy row's Omi, kept across a refresh so its crawl does not restart.
    pub busy: std::rc::Rc<crate::feedback::spinner::Crawl>,
}

impl Card {
    pub(super) fn new(
        entries: Vec<Entry>,
        built: Built,
        frame: Rect,
        cursor: Option<usize>,
    ) -> Self {
        let full_h = frame.h as i32 - 2 * built.metrics.pad;
        let mut card = Self {
            entries,
            built,
            view_h: full_h,
            full_h,
            frame,
            content_y: 0,
            cursor,
            notch: 0,
            touch: 0.,
            busy: Default::default(),
        };
        card.settle();
        card
    }

    pub(super) fn first(entries: &[Entry]) -> Option<usize> {
        usize::try_from(rows::step_row(entries, -1, 1)).ok()
    }

    // The scroll a cursor move lands on, and the view trimmed so a clipped edge cuts a row at half.
    pub(super) fn settle(&mut self) {
        let full = self.full_h;
        let (y, h) = match self.cursor {
            Some(cursor) => {
                self.built
                    .metrics
                    .scroll_for(&self.entries, cursor, self.content_y, full)
            }
            None => self
                .built
                .metrics
                .menu_cut(&self.entries, self.content_y, full, None),
        };
        self.content_y = y;
        self.view_h = h;
        self.frame.h = (h + 2 * self.built.metrics.pad) as f32;
    }

    pub(super) fn step(&mut self, delta: isize) {
        let from = self.cursor.map_or(
            if delta > 0 {
                -1
            } else {
                self.entries.len() as isize
            },
            |at| at as isize,
        );
        if let Ok(at) = usize::try_from(rows::step_row(&self.entries, from, delta)) {
            if at < self.entries.len() {
                self.cursor = Some(at);
            }
        }
        self.settle();
    }

    pub fn row_screen_top(&self, index: usize) -> f32 {
        self.frame.y
            + (self.built.metrics.pad + self.built.metrics.row_top(&self.entries, index)
                - self.content_y) as f32
    }

    pub fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        let f = self.frame;
        if x < f.x
            || x >= f.x + f.w
            || y < f.y + self.built.metrics.pad as f32
            || y >= f.y + (self.built.metrics.pad + self.view_h) as f32
        {
            return None;
        }
        let content = (y - f.y) as i32 - self.built.metrics.pad + self.content_y;
        let mut top = 0;
        for (index, entry) in self.entries.iter().enumerate() {
            top += self.built.metrics.height(entry);
            if content < top {
                return entry.steppable().then_some(index);
            }
        }
        None
    }

    pub(super) fn contains(&self, x: f32, y: f32) -> bool {
        let f = self.frame;
        x >= f.x && x < f.x + f.w && y >= f.y && y < f.y + f.h
    }
}
