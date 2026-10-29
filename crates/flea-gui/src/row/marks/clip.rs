//! The clipboard slot beside a name and the snapshot set behind it: ClipMarks.js, one keyed set per snapshot.
use gpui::{point, px, size, Bounds, Pixels, SharedString};
use std::{cell::Cell, collections::HashSet};

/// The clipboard slot beside a name: copy and scissors share one anchor; a cut ghosts only the file mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipMark {
    Copy,
    Cut,
}

impl ClipMark {
    /// Both glyphs share the reserved strip's edge, one pixel above the line's centre.
    pub fn bounds(self, name: Bounds<Pixels>, side: Pixels, gap: Pixels) -> Bounds<Pixels> {
        Bounds::new(
            point(
                name.right() + gap,
                name.origin.y + (name.size.height - side) / 2. - px(1.),
            ),
            size(side, side),
        )
    }

    pub fn svg(self) -> (SharedString, &'static [u8]) {
        let (key, data) = match self {
            Self::Copy => ("flea-mark:copy", svg!("M9 8h12v13H9z M4 16V3h13")),
            Self::Cut => (
                "flea-mark:scissors",
                svg!("M5 3l14 14 M19 3L5 17 M2 17h5v5H2z M17 17h5v5h-5z"),
            ),
        };
        (SharedString::new_static(key), data)
    }
}

/// ClipMarks.js: one keyed set per clipboard snapshot, scoped to the listing's directory so each row is one lookup by name.
#[derive(Default)]
pub struct ClipMarks {
    paths: Vec<String>,
    moving: bool,
    base: String,
    names: HashSet<String>,
    /// Rows that reached the set; an empty clipboard answers without one, as tests/listcost.qml counts markCalls.
    pub lookups: Cell<u64>,
    /// Moves with every snapshot or scope, so rows resolve their marks once per change rather than once per frame.
    pub revision: u64,
}

impl ClipMarks {
    /// Takes a new clipboard snapshot; an emptied clipboard drops its set at once.
    pub fn set(&mut self, paths: &[String], moving: bool) {
        self.paths.clear();
        self.paths.extend(paths.iter().cloned());
        self.moving = moving;
        self.rebuild();
    }

    /// The directory a listing's names join under, so a search result's relative name resolves against its scope.
    pub fn scope(&mut self, base: &str) {
        if self.base != base {
            self.base.clear();
            self.base.push_str(base);
            self.rebuild();
        }
    }

    fn rebuild(&mut self) {
        self.revision += 1;
        self.names.clear();
        if self.paths.is_empty() {
            self.paths.shrink_to_fit();
            self.names.shrink_to_fit();
            return;
        }
        let base = self.base.trim_end_matches('/');
        for path in &self.paths {
            if let Some(name) = path
                .strip_prefix(base)
                .and_then(|rest| rest.strip_prefix('/'))
            {
                self.names.insert(name.to_owned());
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// The slots this snapshot holds; zero once the clipboard empties.
    pub fn held(&self) -> usize {
        self.paths.capacity() + self.names.capacity()
    }

    /// The mark for a row's name under the scoped directory; an empty clipboard answers without a lookup.
    pub fn mark_for(&self, name: &str) -> Option<ClipMark> {
        if self.paths.is_empty() {
            return None;
        }
        self.lookups.set(self.lookups.get() + 1);
        self.names.contains(name).then_some(if self.moving {
            ClipMark::Cut
        } else {
            ClipMark::Copy
        })
    }
}
