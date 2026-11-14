//! Install bounded size facts before either listing painter reads its rows.
use super::{Held, Size, State, CAP};
use crate::list::{ListState, ListView};
use std::rc::Rc;

impl State {
    pub(super) fn install_held(&mut self, current: &Rc<Held>, list: &mut ListState) {
        self.sizes.inlined(&current.rows, current.start, CAP);
        self.held = Rc::clone(current);
        for (at, _) in current.rows.iter().enumerate() {
            let row = current.index_at(at);
            if let Some(size) = self.sizes.size_for(row) {
                list.view.folder_size(current.listing, row, size);
            }
        }
        list.retain_folder_sizes(&self.sizes);
        self.paint_dirty = false;
    }

    pub(super) fn discard_cancelled(&mut self, list: &mut ListState) {
        if std::mem::take(&mut self.paint_dirty) {
            list.retain_folder_sizes(&self.sizes);
        }
    }
}

impl ListView {
    /// A folder's walked size for one row; a new listing forgets the last one's numbering.
    pub fn folder_size(&mut self, listing: usize, row: usize, size: Size) {
        if listing != self.walked_listing {
            self.walked.clear();
            self.walked_listing = listing;
        }
        if self.walked.insert(row, size) != Some(size) {
            self.walked_revision += 1;
        }
    }

    /// What a cached row's clip mark and walked size were resolved against in this listing.
    pub fn mark_revision(&self, listing: usize) -> crate::row::cache::Revision {
        (self.clip.revision, self.walked_revision, listing)
    }

    pub fn walked_for(&self, listing: usize, row: usize) -> Option<Size> {
        (listing == self.walked_listing)
            .then(|| self.walked.get(&row).copied())
            .flatten()
    }
}
