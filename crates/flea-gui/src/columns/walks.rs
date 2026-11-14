//! Folder walks in the Columns view: the active track's painted rows plan the List's walk, and its size cells read the answers.
use super::paint::FolderSizes;
use crate::{list::ListState, navigation::sort::Size};
use std::{cell::RefCell, rc::Rc};

#[cfg(test)]
#[path = "folder_size_tests.rs"]
mod folder_size_tests;

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
