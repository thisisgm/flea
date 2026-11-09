//! A PDF's page rules for the preview column's continuous stack and Quick Look's single page: layout, the near
//! window, the render width, the document open settle, the slow render fall-back and the viewer's controls.

/// A walk through a folder of documents settles this long before one is opened; an idle press opens on the next frame.
pub const DOCUMENT_SETTLE_MS: u64 = 120;
/// Quick Look's zoom ladder: one step a press, and a bottom rung that always fits the frame.
pub const ZOOM_MIN: f64 = 1.;
pub const ZOOM_MAX: f64 = 4.;
pub const ZOOM_STEP: f64 = 0.25;

/// A page's height over its width, from its point size; 0 for a page with no width.
pub fn ratio(width: f64, height: f64) -> f64 {
    if width > 0. {
        height / width
    } else {
        0.
    }
}

/// A page's width over its height, 1 for a page with no height, so a loading page has a square box.
pub fn aspect(width: f64, height: f64) -> f64 {
    if height > 0. {
        width / height
    } else {
        1.
    }
}

/// Each page's top and height at one width, so the stack is as tall as it will be before any page renders.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub tops: Vec<f64>,
    pub heights: Vec<f64>,
    pub height: f64,
}

impl Layout {
    /// Pages one gap apart at `width`, each as tall as its ratio rounds to; an empty layout for no width.
    pub fn new(ratios: &[f64], width: f64, gap: f64) -> Self {
        let mut layout = Layout::default();
        if width <= 0. {
            return layout;
        }
        let mut y = 0.;
        for ratio in ratios {
            let height = (width * ratio).round();
            layout.tops.push(y);
            layout.heights.push(height);
            y += height + gap;
        }
        layout.height = (y - gap).max(0.);
        layout
    }

    pub fn len(&self) -> usize {
        self.tops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tops.is_empty()
    }

    /// The page whose band, its height and the gap after it, holds the offset; 0 above the first page.
    pub fn page_at(&self, y: f64) -> usize {
        self.tops.partition_point(|top| *top <= y).saturating_sub(1)
    }

    /// The pages within half a frame of the frame, first and last, the last clamped to the document.
    pub fn near(&self, view_top: f64, view_height: f64) -> Option<(usize, usize)> {
        if self.is_empty() {
            return None;
        }
        let first = self.page_at(view_top - view_height / 2.);
        let last = self
            .page_at(view_top + view_height * 1.5)
            .min(self.len() - 1);
        Some((first, last))
    }

    /// The pages any part of the frame shows.
    pub fn in_view(&self, view_top: f64, view_height: f64) -> std::ops::Range<usize> {
        if self.is_empty() {
            return 0..0;
        }
        let first = self.page_at(view_top);
        let last = self.page_at(view_top + view_height).min(self.len() - 1);
        first..last + 1
    }

    /// The offset a turn by `delta` pages from `page` scrolls to, clamped inside the content.
    pub fn turn_offset(&self, page: usize, delta: i64, view_height: f64) -> Option<f64> {
        if self.is_empty() {
            return None;
        }
        let next = clamp_page(page, delta, self.len());
        Some(self.tops[next].min(self.height - view_height).max(0.))
    }
}

/// The page a turn by `delta` lands on, held inside a document of `count` pages.
pub fn clamp_page(page: usize, delta: i64, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    (page as i64 + delta).clamp(0, count as i64 - 1) as usize
}

/// The render width in device pixels, held through small resizes so a drag does not render every pixel.
pub fn render_width(current: u32, target: u32) -> u32 {
    if target == 0 {
        return current;
    }
    if current == 0 {
        return target;
    }
    let change = f64::from(target) / f64::from(current);
    if change > 1.1 || change < 0.9 {
        target
    } else {
        current
    }
}

/// A single page fitted inside the frame without upscaling past the frame: width and height.
pub fn fit(frame_width: f64, frame_height: f64, aspect: f64) -> (f64, f64) {
    (
        frame_width.min(frame_height * aspect),
        frame_height.min(frame_width / aspect),
    )
}

/// One page slot of the continuous stack: near the frame or still rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slot {
    pub page: usize,
    pub rendering: bool,
    pub landed: bool,
    pub errored: bool,
}

impl Slot {
    /// A failed render counts as settled, so the first frame is never held for a page that will not come.
    pub fn settled(&self) -> bool {
        self.landed || self.errored
    }
}

/// The slots are the pages of the near window and any page still rendering; one is added or dropped without moving the others.
pub fn sync_slots(slots: &mut Vec<Slot>, near: Option<(usize, usize)>, count: usize) {
    let Some((first, last)) = near else {
        slots.retain(|slot| slot.rendering && slot.page < count);
        return;
    };
    let last = last.min(count.saturating_sub(1));
    slots.retain(|slot| {
        slot.page < count && ((slot.page >= first && slot.page <= last) || slot.rendering)
    });
    for page in first..=last {
        if count > 0 && !slots.iter().any(|slot| slot.page == page) {
            slots.push(Slot {
                page,
                ..Slot::default()
            });
        }
    }
}

/// The first frame holds until every page it shows has settled; later pages are never waited on.
pub fn first_drawn(slots: &[Slot], layout: &Layout, view_top: f64, view_height: f64) -> bool {
    layout
        .in_view(view_top, view_height)
        .all(|page| slots.iter().any(|slot| slot.page == page && slot.settled()))
}

#[path = "pdf_timing.rs"]
mod timing;
pub use timing::{due, open_now, Cap, Fetch, Opener};

#[cfg(test)]
#[path = "pdf_tests.rs"]
mod tests;
