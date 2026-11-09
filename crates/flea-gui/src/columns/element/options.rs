//! The track's optional inputs, set by the window before the element paints.
use super::TrackElement;
use crate::columns::paint::FolderSizes;
use crate::grid::element::pictures::Pictures;
use std::rc::Rc;

impl TrackElement {
    /// ColumnRow's language marks follow the List's langMarks setting.
    pub fn with_languages(mut self, languages: bool) -> Self {
        self.languages = languages;
        self
    }

    /// A tagged folder reserves the same slot on every row of its active column.
    pub fn with_tag_slot(mut self, shown: bool) -> Self {
        self.tag_slot = shown;
        self
    }

    /// A standing filter's drawn rows: the active track then draws the listing row at each view position.
    pub fn with_filter(mut self, shown: Option<Rc<[usize]>>) -> Self {
        self.shown = shown;
        self
    }

    pub fn with_location(mut self, locating: bool) -> Self {
        self.locating = locating;
        self
    }

    pub fn with_query(mut self, query: Rc<str>) -> Self {
        self.query = query;
        self
    }

    /// The folder walks the active track's size cells read; a peek track never shows a size.
    pub fn with_sizes(mut self, sizes: Rc<dyn FolderSizes>) -> Self {
        self.sizes = Some(sizes);
        self
    }

    /// The pane's thumbnail lender, which only the active track's listing rows draw from.
    pub fn with_pictures(mut self, pictures: Rc<Pictures>) -> Self {
        self.pictures = Some(pictures);
        self
    }
}

impl TrackElement {
    /// The row pitch and the band each row centres its marks and lines in.
    pub(crate) fn with_rows(mut self, row_height: f32, band: f32) -> Self {
        self.row_height = row_height;
        self.band = band;
        self
    }
}
