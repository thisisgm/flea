//! Folder walks in the Columns view: the active track's painted rows plan the List's walk, and its size cells read the answers.
use super::{geometry::Slot, paint::FolderSizes};
use crate::{list::ListState, navigation::sort::Size, window::pane::PaneHost};
use std::{cell::RefCell, rc::Rc};

#[cfg(test)]
#[path = "folder_size_tests.rs"]
mod folder_size_tests;

/// The first and last listing rows the active track last painted, when that paint drew this listing.
pub(crate) fn painted_rows(host: &PaneHost, listing: usize) -> Option<(usize, usize)> {
    let painter = host.columns.as_ref()?.painters.get(Slot::Active)?.borrow();
    if painter.drawn.as_ref()?.held.as_ref()?.listing != listing {
        return None;
    }
    let mut rows = painter.visible();
    let first = rows.next()?;
    Some((first, rows.last().unwrap_or(first)))
}

/// The pane's answered walks as the List keeps them, read per row at prepaint; ColumnPane.qml reads pane.dirSizeState the same way.
pub(crate) struct Walked {
    pub list: Rc<RefCell<ListState>>,
    pub listing: usize,
}

impl FolderSizes for Walked {
    fn size_for(&self, index: usize) -> Option<Size> {
        self.list
            .try_borrow()
            .ok()?
            .view
            .walked_for(self.listing, index)
    }
}
