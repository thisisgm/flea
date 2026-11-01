//! The read-only cursor and selection facts the Grid borrows from its pane owner.
/// What the focus and selection owner tells the grid each frame; the grid reads it and never writes it.
pub trait FocusFacts {
    /// The cursor's listing row.
    fn cursor(&self) -> usize;
    /// False while another chooser control holds the keyboard.
    fn focused(&self) -> bool {
        true
    }
    /// A chooser lends its theme-eased selection plate; other hosts keep their existing recipe.
    fn selection_fill(&self) -> Option<gpui::Hsla> {
        None
    }
    fn selected(&self, listing: usize) -> bool;
    /// False while a band sweeps, so hover draws no wash.
    fn hover_shown(&self) -> bool {
        true
    }
}

/// A plain copy of the focus facts, for a host that has no live owner to lend.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusSnapshot {
    pub cursor: usize,
    /// Ascending, so membership is a binary search.
    pub selected: Vec<usize>,
    pub band: bool,
    /// Another control holds the keyboard, so the cursor keeps its grey plate without an accent frame.
    pub grey: bool,
}

impl FocusFacts for FocusSnapshot {
    fn cursor(&self) -> usize {
        self.cursor
    }
    fn focused(&self) -> bool {
        !self.grey
    }
    fn selected(&self, listing: usize) -> bool {
        self.selected.binary_search(&listing).is_ok()
    }
    fn hover_shown(&self) -> bool {
        !self.band
    }
}
