//! A bounded LRU of prepared rows: a row entering the viewport is shaped once, a visible row never again.
use super::{
    marks::{ClipMark, Mark},
    NameInk,
};
use crate::navigation::sort::Size;
use flea::protocol::RowFacts;
use gpui::{Font, GlyphRun, LineLayout, Pixels};
use std::{cell::OnceCell, collections::HashMap, rc::Rc, sync::Arc};

/// One row's shaped cells as glyph runs whose tiles the window keeps; colour is chosen at paint, so the cursor reshapes nothing.
pub struct Prepared {
    pub label: String,
    pub size_text: String,
    pub date_text: String,
    pub location: GlyphRun,
    pub name: GlyphRun,
    pub size: GlyphRun,
    pub date: GlyphRun,
    pub mode: GlyphRun,
    pub kind: GlyphRun,
    pub mark: Mark,
    pub ink: NameInk,
    /// The match run's byte range inside the drawn name, when a filter or search marks one.
    pub run: Option<(usize, usize)>,
    /// The name with its run in bold, shaped the first time the cursor rests on this row.
    pub bold: OnceCell<GlyphRun>,
}

impl Default for Prepared {
    fn default() -> Self {
        let empty = Arc::new(LineLayout::default());
        Self {
            label: String::new(),
            size_text: String::new(),
            date_text: String::new(),
            location: GlyphRun::new(Arc::clone(&empty)),
            name: GlyphRun::new(Arc::clone(&empty)),
            size: GlyphRun::new(Arc::clone(&empty)),
            date: GlyphRun::new(Arc::clone(&empty)),
            mode: GlyphRun::new(Arc::clone(&empty)),
            kind: GlyphRun::new(empty),
            mark: Mark::File,
            ink: NameInk::Foreground,
            run: None,
            bold: OnceCell::new(),
        }
    }
}

/// What every cached row was shaped for: the face, the name and cell budgets, the scale and the text revision; any change drops them all.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shaping {
    pub listing: usize,
    pub font: Font,
    pub body: Pixels,
    pub caption: Pixels,
    pub name_cells: usize,
    /// The body face's digit advance, one cell's pixels; zero leaves names to the cell recipe alone.
    pub advance: f32,
    /// The name's budget on a row whose clip mark takes the gap and the mark beside it.
    pub clip_cells: usize,
    /// Each column's budget in caption cells, zero where the column is not drawn.
    pub cells: [usize; 4],
    pub scale: f32,
    pub text: u64,
    pub languages: bool,
}

/// What besides its facts one row was shaped from: a folder's walked size, whether a clip mark narrows its name, and a drop caption's name budget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extra {
    pub walked: Option<Size>,
    pub clipped: bool,
    pub drop: Option<usize>,
}

// A revision no clipboard or walk ever has, so a row shaped without one always resolves its marks again.
const UNRESOLVED: Revision = (u64::MAX, u64::MAX, usize::MAX);

/// The clipboard's revision, the walk's revision and the listing a row's marks were resolved against.
pub type Revision = (u64, u64, usize);

// A prepared row with the facts it was shaped from, since a window refresh re-states rows within one listing.
struct Entry {
    facts: RowFacts,
    extra: Extra,
    prepared: Rc<Prepared>,
    used: u64,
    revision: Revision,
    clip: Option<ClipMark>,
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
    /// Retain prepared cells by file identity when an in-place answer renumbers rows.
    pub(crate) fn reindex(&mut self, held: &crate::list::pane::Held) {
        if let Some(shaping) = self.shaping.as_mut() {
            shaping.listing = held.listing;
        }
        self.entries = std::mem::take(&mut self.entries)
            .into_values()
            .filter_map(|mut entry| {
                let at = held
                    .rows
                    .iter()
                    .position(|row| row.name == entry.facts.name)?;
                entry.revision = UNRESOLVED;
                Some((held.start + at, entry))
            })
            .collect();
    }

    /// The facts retained for a prepared row, so observations never mix its frame with a newer reply.
    pub(crate) fn facts(&self, index: usize) -> Option<&RowFacts> {
        self.entries.get(&index).map(|entry| &entry.facts)
    }

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
    pub fn get_or_prepare(
        &mut self,
        index: usize,
        facts: &RowFacts,
        prepare: impl FnOnce() -> Prepared,
    ) -> Rc<Prepared> {
        self.get_or_prepare_with(index, facts, Extra::default(), prepare)
    }

    /// The same, for a row whose walked size or clip mark also shapes it.
    pub fn get_or_prepare_with(
        &mut self,
        index: usize,
        facts: &RowFacts,
        extra: Extra,
        prepare: impl FnOnce() -> Prepared,
    ) -> Rc<Prepared> {
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(&index) {
            if entry.facts == *facts && entry.extra == extra {
                entry.used = self.clock;
                self.hits += 1;
                return Rc::clone(&entry.prepared);
            }
            self.misses += 1;
            let prepared = Rc::new(prepare());
            entry.facts.clone_from(facts);
            entry.extra = extra;
            entry.prepared = Rc::clone(&prepared);
            entry.used = self.clock;
            entry.revision = UNRESOLVED;
            return prepared;
        }
        self.misses += 1;
        if self.entries.len() >= self.capacity {
            self.evict();
        }
        let prepared = Rc::new(prepare());
        let entry = Entry {
            facts: facts.clone(),
            extra,
            prepared: Rc::clone(&prepared),
            used: self.clock,
            revision: UNRESOLVED,
            clip: None,
        };
        self.entries.insert(index, entry);
        self.peak = self.peak.max(self.entries.len());
        prepared
    }

    /// The same, resolving a row's clip mark and walked size only when the clipboard or walk revision moved or the row is new.
    /// A drop target's name budget comes as (plain, clipped), taken by whether the row's clip mark draws.
    pub fn get_or_prepare_marked(
        &mut self,
        index: usize,
        facts: &RowFacts,
        revision: Revision,
        drop: Option<(usize, usize)>,
        resolve: impl FnOnce() -> (Extra, Option<ClipMark>),
        prepare: impl FnOnce(Extra) -> Prepared,
    ) -> (Rc<Prepared>, Extra, Option<ClipMark>) {
        if let Some(entry) = self.entries.get_mut(&index) {
            let budget =
                drop.map(|(plain, clipped)| if entry.clip.is_some() { clipped } else { plain });
            if entry.revision == revision && entry.extra.drop == budget && entry.facts == *facts {
                self.clock += 1;
                entry.used = self.clock;
                self.hits += 1;
                return (Rc::clone(&entry.prepared), entry.extra, entry.clip);
            }
        }
        let (resolved, clip) = resolve();
        let drop = drop.map(|(plain, clipped)| if clip.is_some() { clipped } else { plain });
        let extra = Extra { drop, ..resolved };
        let prepared = self.get_or_prepare_with(index, facts, extra, || prepare(extra));
        if let Some(entry) = self.entries.get_mut(&index) {
            entry.revision = revision;
            entry.clip = clip;
        }
        (prepared, extra, clip)
    }

    /// Rows shaped and held now, read by the bench seam's report, IPC and the tests.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn evict(&mut self) {
        if let Some(oldest) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.used)
            .map(|(index, _)| *index)
        {
            self.entries.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{font, px};

    fn shaping(listing: usize) -> Shaping {
        Shaping {
            listing,
            font: font("JetBrains Mono"),
            body: px(14.),
            caption: px(12.),
            name_cells: 40,
            scale: 1.,
            ..Shaping::default()
        }
    }

    fn prepared() -> Prepared {
        Prepared::default()
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
        assert!(Rc::ptr_eq(
            &kept,
            &cache.get_or_prepare(9_999, &row, prepared)
        ));
        assert_eq!(cache.hits, 2);
        cache.begin(&shaping(1), 30);
        assert_eq!(cache.len(), 30);
        cache.get_or_prepare(9_999, &row, || {
            panic!("the newest row was evicted before older ones")
        });
        cache.begin(&shaping(2), 30);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn a_row_whose_facts_changed_in_the_same_listing_is_shaped_again_and_an_equal_copy_is_not() {
        let mut cache = RowCache::default();
        cache.begin(&shaping(1), 87);
        let first = cache.get_or_prepare(0, &facts("a.txt"), prepared);
        let fresh_copy = facts("a.txt");
        let reused =
            cache.get_or_prepare(0, &fresh_copy, || panic!("equal facts were shaped again"));
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
            assert!(
                !Rc::ptr_eq(&last, &shaped),
                "change {step} reused the row shaped from older facts"
            );
            assert!(Rc::ptr_eq(
                &shaped,
                &cache.get_or_prepare(0, &row, || panic!("change {step} shaped twice"))
            ));
            last = cache.get_or_prepare(0, &facts("a.txt"), prepared);
        }
        assert_eq!(cache.len(), 1, "a reshaped row took a second entry");
        assert_eq!(
            (cache.misses, cache.hits),
            (1 + 2 * changes.len() as u64, 1 + changes.len() as u64)
        );
    }

    #[test]
    fn a_walked_size_or_a_clip_mark_reshapes_its_row_and_nothing_else() {
        let mut cache = RowCache::default();
        cache.begin(&shaping(1), 87);
        let folder = RowFacts {
            dir: true,
            ..facts("Docs")
        };
        let first = cache.get_or_prepare_with(0, &folder, Extra::default(), prepared);
        let walked = Extra {
            walked: Some(Size {
                bytes: 4_096,
                partial: true,
            }),
            clipped: false,
            drop: None,
        };
        let sized = cache.get_or_prepare_with(0, &folder, walked, prepared);
        assert!(
            !Rc::ptr_eq(&first, &sized),
            "a folder kept its dot after its walk answered"
        );
        let clipped = Extra {
            clipped: true,
            ..walked
        };
        let narrowed = cache.get_or_prepare_with(0, &folder, clipped, prepared);
        assert!(
            !Rc::ptr_eq(&sized, &narrowed),
            "a clip mark did not narrow the name"
        );
        let again = cache.get_or_prepare_with(0, &folder, clipped, || {
            panic!("an equal row was shaped again")
        });
        assert!(Rc::ptr_eq(&narrowed, &again));
        assert_eq!(cache.len(), 1);
    }
}
