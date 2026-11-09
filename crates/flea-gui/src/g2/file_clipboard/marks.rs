//! The clipboard's row marks as the Columns tracks and Grid tiles read them: one lookup per selection revision, one reused join buffer, so a warm frame allocates nothing.
use crate::{columns::paint, grid::element, row::marks::ClipMark};
use flea::protocol::RowFacts;
use flea::ui_logic_file_clipboard::{self as core};
use std::{cell::RefCell, rc::Rc};

/// One selection's marks; a new revision builds a new set, an emptied clipboard answers before any join.
pub struct ViewMarks {
    marks: RefCell<core::Marks>,
    probe: RefCell<String>,
    empty: bool,
    cut: bool,
}

impl ViewMarks {
    pub fn new(revision: u64, clip: &core::Clip) -> Self {
        let mut marks = core::Marks::new();
        if !clip.paths.is_empty() {
            marks.set_for(revision, &clip.paths, clip.moving);
        }
        Self {
            marks: RefCell::new(marks),
            probe: RefCell::new(String::new()),
            empty: clip.paths.is_empty(),
            cut: clip.moving,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.empty
    }

    // Sample input: dir "/w" and name "a.txt" with "/w/a.txt" cut reads Some(Cut).
    pub fn mark(&self, dir: &str, name: &str) -> Option<ClipMark> {
        if self.empty || name.is_empty() {
            return None;
        }
        let mut probe = self.probe.borrow_mut();
        core::row_path_into(&mut probe, dir, name);
        match self.marks.borrow_mut().mark_for(&probe) {
            "" => None,
            _ if self.cut => Some(ClipMark::Cut),
            _ => Some(ClipMark::Copy),
        }
    }
}

impl paint::ClipMarks for ViewMarks {
    fn is_empty(&self) -> bool {
        self.empty
    }

    fn mark(&self, dir: &str, name: &str) -> paint::Clip {
        match ViewMarks::mark(self, dir, name) {
            Some(ClipMark::Cut) => paint::Clip::Cut,
            Some(ClipMark::Copy) => paint::Clip::Copy,
            None => paint::Clip::None,
        }
    }
}

/// The Grid's decor: the pane folder its tile names join under, and the shared selection marks.
pub struct GridMarks {
    pub marks: Rc<ViewMarks>,
    pub base: String,
}

impl GridMarks {
    // Pictures draws the marks through itself once installed, so neither decor ever drops the other.
    pub(crate) fn lend(grid: &mut crate::grid::host::GridHost, marks: Rc<ViewMarks>, base: String) {
        let decor: Rc<dyn element::Decor> = Rc::new(GridMarks { marks, base });
        match grid.decor.pictures() {
            Some(pictures) => pictures.set_clip(decor),
            None => grid.decor = decor,
        }
    }
}

impl element::Decor for GridMarks {
    fn clip_row(&self, _listing: usize, row: Option<&RowFacts>) -> Option<element::Clip> {
        let mark = self.marks.mark(&self.base, &row?.name)?;
        let (key, svg) = mark.svg();
        Some(element::Clip {
            cut: mark == ClipMark::Cut,
            key,
            svg,
        })
    }
}
