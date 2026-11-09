//! The rendered pages a surface holds: a few pages per document by width, oldest dropped first, each drop handed to paint.
use gpui::RenderImage;
use std::sync::Arc;

/// One rendered page at one width in device pixels.
pub struct Entry {
    pub page: usize,
    pub width: u32,
    pub image: Arc<RenderImage>,
}

/// A bounded store of rendered pages, most recently used last; what leaves it is released from the atlas at the next paint.
pub struct PageCache {
    entries: Vec<Entry>,
    limit: usize,
    released: Vec<Arc<RenderImage>>,
}

impl PageCache {
    pub fn new(limit: usize) -> Self {
        PageCache {
            entries: Vec::with_capacity(limit + 1),
            limit,
            released: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Enough room for the pages a frame shows, so the cap never drops a page on screen.
    pub fn set_limit(&mut self, limit: usize, keep: &[usize]) {
        self.limit = limit.max(1);
        self.trim(keep);
    }

    /// The page at exactly this width.
    pub fn exact(&self, page: usize, width: u32) -> Option<&Arc<RenderImage>> {
        self.entries
            .iter()
            .find(|e| e.page == page && e.width == width)
            .map(|e| &e.image)
    }

    /// The page at the width closest to this one, which stands in while the exact one renders.
    pub fn nearest(&self, page: usize, width: u32) -> Option<&Arc<RenderImage>> {
        self.entries
            .iter()
            .filter(|e| e.page == page)
            .min_by_key(|e| e.width.abs_diff(width))
            .map(|e| &e.image)
    }

    /// A landed page; a page kept wanted by `keep` is never the one trimmed.
    pub fn insert(&mut self, page: usize, width: u32, image: Arc<RenderImage>, keep: &[usize]) {
        if let Some(at) = self
            .entries
            .iter()
            .position(|e| e.page == page && e.width == width)
        {
            let old = self.entries.remove(at);
            self.released.push(old.image);
        }
        // An older width of the same page is replaced, never kept beside it.
        let mut at = 0;
        while at < self.entries.len() {
            if self.entries[at].page == page {
                let old = self.entries.remove(at);
                self.released.push(old.image);
            } else {
                at += 1;
            }
        }
        self.entries.push(Entry { page, width, image });
        self.trim(keep);
    }

    /// Marks a page as used now, so it is the last to leave.
    pub fn touch(&mut self, page: usize) {
        if let Some(at) = self.entries.iter().position(|e| e.page == page) {
            let entry = self.entries.remove(at);
            self.entries.push(entry);
        }
    }

    fn trim(&mut self, keep: &[usize]) {
        while self.entries.len() > self.limit {
            let Some(at) = self.entries.iter().position(|e| !keep.contains(&e.page)) else {
                return;
            };
            let old = self.entries.remove(at);
            self.released.push(old.image);
        }
    }

    /// Another document's pages are never this one's: every page leaves.
    pub fn clear(&mut self) {
        self.released
            .extend(self.entries.drain(..).map(|e| e.image));
    }

    /// Every image held or waiting for release, for a view that is going away.
    pub fn drain(&mut self) -> Vec<Arc<RenderImage>> {
        self.clear();
        self.take_released()
    }

    /// Images to release from the atlas, taken once by paint or by a reply applied outside a window's update.
    pub fn take_released(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.released)
    }

    /// An image that landed for a document no longer open, released with the rest.
    pub fn release(&mut self, image: Arc<RenderImage>) {
        self.released.push(image);
    }

    pub fn pages(&self) -> impl Iterator<Item = usize> + '_ {
        self.entries.iter().map(|e| e.page)
    }

    /// The bytes the held pages take, four a pixel.
    pub fn bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|e| {
                let size = e.image.size(0);
                size.width.0 as usize * size.height.0 as usize * 4
            })
            .sum()
    }
}
