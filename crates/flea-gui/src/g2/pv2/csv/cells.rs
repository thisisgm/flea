//! A bounded LRU of shaped table cells: a cell entering the frame is shaped once, and a hit allocates nothing.
use flea::ui_logic_pv2::csv_feed::Face;
use gpui::{GlyphRun, SharedString};
use std::{collections::HashMap, rc::Rc};

/// The header's record ordinal, and the column a row number takes.
pub const HEADER: usize = 1;
pub const NUMBER: usize = usize::MAX;

struct Entry {
    source: Box<str>,
    fit: usize,
    face: Face,
    text: Rc<str>,
    cut: bool,
    run: Rc<GlyphRun>,
    used: u64,
}

/// A shaped cell: the text it draws, whether it was cut, and its run.
pub struct Shaped {
    pub text: Rc<str>,
    pub cut: bool,
    pub run: Rc<GlyphRun>,
}

/// Shaped cells by record, column and drawn window; any change of face, size or scale drops them all.
#[derive(Default)]
pub struct CellCache {
    shaping: Option<(SharedString, u32, u32)>,
    entries: HashMap<(usize, usize, usize, usize), Entry>,
    clock: u64,
    capacity: usize,
    /// Cells shaped and cells reused, which the cost tests count.
    pub shaped: usize,
    pub hits: usize,
}

impl Entry {
    fn shaped(&self) -> Shaped {
        Shaped {
            text: Rc::clone(&self.text),
            cut: self.cut,
            run: Rc::clone(&self.run),
        }
    }
}

impl CellCache {
    /// Starts a frame: a new shaping clears the cells, and the bound follows the frame's cells.
    pub fn begin(&mut self, family: &SharedString, caption: f32, scale: f32, capacity: usize) {
        let (caption, scale) = (caption.to_bits(), scale.to_bits());
        if !matches!(&self.shaping, Some((f, c, s)) if f == family && *c == caption && *s == scale)
        {
            self.entries.clear();
            self.shaping = Some((family.clone(), caption, scale));
        }
        self.capacity = capacity.max(1);
        while self.entries.len() > self.capacity {
            self.evict();
        }
    }

    /// A new subject's cells are never this one's.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The cell shaped from exactly this source, fit, face and drawn window; anything else `shape` draws again.
    /// A whole cell keys (record, column, 0, 0) and a window its 1-based [start, end), so the two never share a key.
    pub fn get_or_shape(
        &mut self,
        at: (usize, usize, usize, usize),
        source: &str,
        fit: usize,
        face: Face,
        shape: impl FnOnce() -> (String, bool, GlyphRun),
    ) -> Shaped {
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(&at) {
            if *entry.source == *source && entry.fit == fit && entry.face == face {
                entry.used = self.clock;
                self.hits += 1;
                return entry.shaped();
            }
        } else if self.entries.len() >= self.capacity {
            self.evict();
        }
        self.shaped += 1;
        let (text, cut, run) = shape();
        let entry = Entry {
            source: source.into(),
            fit,
            face,
            text: text.into(),
            cut,
            run: Rc::new(run),
            used: self.clock,
        };
        let shaped = entry.shaped();
        self.entries.insert(at, entry);
        shaped
    }

    fn evict(&mut self) {
        // A linear scan: the bound is a few frames of visible cells, and only a miss evicts.
        if let Some(oldest) = self
            .entries
            .iter()
            .min_by_key(|(_, e)| e.used)
            .map(|(at, _)| *at)
        {
            self.entries.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::LineLayout;
    use std::sync::Arc;

    fn run() -> (String, bool, GlyphRun) {
        (
            "x".into(),
            false,
            GlyphRun::new(Arc::new(LineLayout::default())),
        )
    }

    #[test]
    fn cells_stay_bounded_and_reshape_only_on_a_change() {
        let mut cache = CellCache::default();
        let family = SharedString::from("JetBrains Mono");
        cache.begin(&family, 12., 1., 40);
        for record in 2..1000 {
            cache.get_or_shape((record, 0, 0, 0), "x", 9, Face::Body, run);
            assert!(cache.len() <= 40);
        }
        assert_eq!((cache.shaped, cache.hits), (998, 0));
        cache.get_or_shape((999, 0, 0, 0), "x", 9, Face::Body, || {
            panic!("a held cell was shaped again")
        });
        let again = cache.get_or_shape((999, 0, 0, 0), "y", 9, Face::Body, run);
        assert_eq!(cache.shaped, 999, "a changed source is shaped again");
        let hit = cache.get_or_shape((999, 0, 0, 0), "y", 9, Face::Body, || panic!("reshaped"));
        assert!(Rc::ptr_eq(&again.run, &hit.run) && Rc::ptr_eq(&again.text, &hit.text));
        cache.get_or_shape((999, 0, 0, 0), "y", 4, Face::Body, run);
        assert_eq!(cache.shaped, 1000, "a changed fit is shaped again");
        cache.get_or_shape((999, 0, 0, 0), "y", 4, Face::Figure, run);
        assert_eq!(cache.shaped, 1001, "a changed face is shaped again");
        cache.get_or_shape((999, 0, 1, 7), "y", 4, Face::Figure, run);
        assert_eq!(cache.shaped, 1002, "a drawn window is shaped again");
        let whole = cache.get_or_shape((999, 0, 0, 0), "y", 4, Face::Figure, || panic!("reshaped"));
        let window =
            cache.get_or_shape((999, 0, 1, 7), "y", 4, Face::Figure, || panic!("reshaped"));
        assert_eq!(
            cache.shaped, 1002,
            "both the whole cell and its window are held"
        );
        assert!(
            !Rc::ptr_eq(&whole.run, &window.run),
            "a window never shares the whole cell"
        );
        cache.begin(&family, 12., 1., 40);
        assert_eq!(cache.len(), 40, "the same shaping keeps every cell");
        cache.begin(&family, 12., 2., 40);
        assert!(cache.is_empty(), "a new scale drops every cell");
    }
}
