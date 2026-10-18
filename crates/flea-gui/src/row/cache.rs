//! A bounded LRU of prepared rows: a row entering the viewport is shaped once, a visible row never again.
use super::{marks::Mark, NameInk};
use flea::protocol::RowFacts;
use gpui::{Font, GlyphRun, Pixels};
use std::{collections::HashMap, rc::Rc};

/// One row's shaped cells as glyph runs whose tiles the window keeps; colour is chosen at paint, so the cursor reshapes nothing.
pub struct Prepared {
    pub label: String,
    pub size_text: String,
    pub date_text: String,
    pub name: GlyphRun,
    pub size: GlyphRun,
    pub date: GlyphRun,
    pub mark: Mark,
    pub ink: NameInk,
}

/// What every cached row was shaped for; any change drops them all.
#[derive(Clone, Debug, PartialEq)]
pub struct Shaping {
    pub listing: usize,
    pub font: Font,
    pub body: Pixels,
    pub caption: Pixels,
    pub name_cells: usize,
    pub scale: f32,
}

// A prepared row with the facts it was shaped from, since a window refresh re-states rows within one listing.
struct Entry {
    facts: RowFacts,
    prepared: Rc<Prepared>,
    used: u64,
}

#[derive(Default)]
pub struct RowCache {
    shaping: Option<Shaping>,
    entries: HashMap<usize, Entry>,
    clock: u64,
    capacity: usize,
    pub hits: u64,
    pub misses: u64,
    pub peak: usize,
}

impl RowCache {
    /// Starts a frame: a new shaping clears the rows, and the bound follows the viewport.
    pub fn begin(&mut self, shaping: &Shaping, capacity: usize) {
        if self.shaping.as_ref() != Some(shaping) {
            self.entries.clear();
            self.shaping = Some(shaping.clone());
        }
        self.capacity = capacity.max(1);
        while self.entries.len() > self.capacity {
            self.evict();
        }
    }

    /// The row shaped from exactly these facts: an entry whose facts differ is shaped again, an equal one is reused as is.
    pub fn get_or_prepare(&mut self, index: usize, facts: &RowFacts, prepare: impl FnOnce() -> Prepared) -> Rc<Prepared> {
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(&index) {
            if entry.facts == *facts {
                entry.used = self.clock;
                self.hits += 1;
                return Rc::clone(&entry.prepared);
            }
            self.misses += 1;
            let prepared = Rc::new(prepare());
            entry.facts.clone_from(facts);
            entry.prepared = Rc::clone(&prepared);
            entry.used = self.clock;
            return prepared;
        }
        self.misses += 1;
        if self.entries.len() >= self.capacity {
            self.evict();
        }
        let prepared = Rc::new(prepare());
        let entry = Entry {
            facts: facts.clone(),
            prepared: Rc::clone(&prepared),
            used: self.clock,
        };
        self.entries.insert(index, entry);
        self.peak = self.peak.max(self.entries.len());
        prepared
    }

    // Only the bench seam's report and the tests read the live count.
    #[cfg(any(test, feature = "bench-seam"))]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    fn evict(&mut self) {
        if let Some(oldest) = self.entries.iter().min_by_key(|(_, entry)| entry.used).map(|(index, _)| *index) {
            self.entries.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{font, px, LineLayout};
    use std::sync::Arc;

    fn shaping(listing: usize) -> Shaping {
        Shaping {
            listing,
            font: font("JetBrains Mono"),
            body: px(14.),
            caption: px(12.),
            name_cells: 40,
            scale: 1.,
        }
    }

    fn prepared() -> Prepared {
        let layout = Arc::new(LineLayout::default());
        Prepared {
            label: String::new(),
            size_text: String::new(),
            date_text: String::new(),            name: GlyphRun::new(Arc::clone(&layout)),
            size: GlyphRun::new(Arc::clone(&layout)),
            date: GlyphRun::new(layout),
            mark: Mark::File,
            ink: NameInk::Foreground,
        }
    }

    fn facts(name: &str) -> RowFacts {
        RowFacts {
            name: name.into(),
            size: 26,
            mtime: 1_760_600_000,
            mode: 0o100644,
            icon: "text-plain".into(),
            real_modes: true,
            ..RowFacts::default()
        }
    }

    #[test]
    fn the_cache_stays_bounded_reuses_rows_and_drops_them_on_a_new_shaping() {
        let mut cache = RowCache::default();
        let row = facts("a.txt");
        cache.begin(&shaping(1), 87);
        for index in 0..10_000 {
            cache.get_or_prepare(index, &row, prepared);
            assert!(cache.len() <= 87);
        }
        assert_eq!((cache.misses, cache.hits, cache.peak), (10_000, 0, 87));
        let kept = cache.get_or_prepare(9_999, &row, || panic!("a cached row was shaped again"));
        assert!(Rc::ptr_eq(&kept, &cache.get_or_prepare(9_999, &row, prepared)));
        assert_eq!(cache.hits, 2);
        cache.begin(&shaping(1), 30);
        assert_eq!(cache.len(), 30);
        cache.get_or_prepare(9_999, &row, || panic!("the newest row was evicted before older ones"));
        cache.begin(&shaping(2), 30);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn a_row_whose_facts_changed_in_the_same_listing_is_shaped_again_and_an_equal_copy_is_not() {
        let mut cache = RowCache::default();
        cache.begin(&shaping(1), 87);
        let first = cache.get_or_prepare(0, &facts("a.txt"), prepared);
        let fresh_copy = facts("a.txt");
        let reused = cache.get_or_prepare(0, &fresh_copy, || panic!("equal facts were shaped again"));
        assert!(Rc::ptr_eq(&first, &reused));
        let changes: [fn(&mut RowFacts); 7] = [
            |row| row.size = 12_345_678,
            |row| row.mtime += 60,
            |row| row.mode = 0o100755,
            |row| row.icon = "image-x-generic".into(),
            |row| row.link = Some("target".into()),
            |row| row.real_modes = false,
            |row| row.dir = true,
        ];
        let mut last = reused;
        for (step, change) in changes.iter().enumerate() {
            let mut row = facts("a.txt");
            change(&mut row);
            let shaped = cache.get_or_prepare(0, &row, prepared);
            assert!(!Rc::ptr_eq(&last, &shaped), "change {step} reused the row shaped from older facts");
            assert!(Rc::ptr_eq(&shaped, &cache.get_or_prepare(0, &row, || panic!("change {step} shaped twice"))));
            last = cache.get_or_prepare(0, &facts("a.txt"), prepared);
        }
        assert_eq!(cache.len(), 1, "a reshaped row took a second entry");
        assert_eq!((cache.misses, cache.hits), (1 + 2 * changes.len() as u64, 1 + changes.len() as u64));
    }
}
