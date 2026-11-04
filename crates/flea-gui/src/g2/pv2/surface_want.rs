//! What the surface asks the renderer for: the pages in reach, the first frame's settle and the cache's keep list.
use super::{Command, Surface};
use flea::ui_logic_pv2::pdf;

impl Surface {
    // The device width a page renders at: the stack's width, that page fitted inside the frame, or a neighbour fitted unzoomed.
    pub(super) fn target(&self, page: usize) -> u32 {
        let (w, h) = if page != self.page && self.fitted.0 > 0. {
            self.fitted
        } else {
            (self.width, self.view_height)
        };
        let width = if self.continuous {
            self.width
        } else {
            pdf::fit(w, h, self.aspect_of(page)).0
        };
        (width * self.scale).ceil() as u32
    }

    // Before the first draw the top page failing fails the stack, and the frame draws once every page in view has settled.
    pub(super) fn first_frame(&mut self) {
        if self.cap.shown_page.is_some() || self.layout.is_empty() {
            return;
        }
        let top = self.layout.page_at(self.view_top);
        if self
            .slots
            .iter()
            .any(|slot| slot.page == top && slot.errored)
        {
            self.page_failed = true;
        }
        if pdf::first_drawn(&self.slots, &self.layout, self.view_top, self.view_height) {
            self.cap.shown_page = Some(top);
        }
    }

    // The pages the cache must not drop: the stack's near window, or the single page and its neighbours.
    pub(super) fn kept(&self) -> Vec<usize> {
        if self.continuous {
            self.slots.iter().map(|slot| slot.page).collect()
        } else {
            self.neighbours()
        }
    }

    // Asks for each wanted page not held, not rendering and not failed; renders for pages left behind are dropped.
    pub(super) fn want(&mut self) {
        let Some(doc) = self.doc else { return };
        if self.sizes.is_empty() || self.render_width == 0 {
            return;
        }
        let pages: Vec<usize> = if self.continuous {
            let near = self.layout.near(self.view_top, self.view_height);
            for slot in &mut self.slots {
                slot.rendering &=
                    near.is_some_and(|(first, last)| (first..=last).contains(&slot.page));
            }
            pdf::sync_slots(&mut self.slots, near, self.sizes.len());
            for slot in &mut self.slots {
                slot.errored |= self.failed_pages.contains(&slot.page);
                slot.landed |= self.cache.exact(slot.page, self.render_width).is_some();
            }
            self.first_frame();
            let keep = self.kept();
            self.cache.set_limit(self.slots.len() + 2, &keep);
            keep
        } else {
            self.neighbours()
        };
        let wanted: Vec<(usize, u32)> = pages
            .into_iter()
            .filter(|page| !self.failed_pages.contains(page))
            .map(|page| {
                (
                    page,
                    if self.continuous || page == self.page {
                        self.render_width
                    } else {
                        self.target(page)
                    },
                )
            })
            .collect();
        self.inflight.retain(|pair| wanted.contains(pair));
        if wanted != self.wanted {
            self.wanted.clone_from(&wanted);
            self.commands.push(Command::Want {
                doc,
                pages: wanted.clone(),
            });
        }
        for (page, width) in wanted {
            if self.cache.exact(page, width).is_some() || self.inflight.contains(&(page, width)) {
                continue;
            }
            self.inflight.push((page, width));
            self.counters.asked += 1;
            self.commands.push(Command::Render { doc, page, width });
            if let Some(slot) = self.slots.iter_mut().find(|slot| slot.page == page) {
                slot.rendering = true;
            }
        }
    }

    // The single page first, then its neighbours at their own widths, so a turn lands from the cache.
    pub(super) fn neighbours(&self) -> Vec<usize> {
        let count = self.sizes.len();
        let mut pages = vec![self.page];
        pages.extend(
            [self.page + 1, self.page.wrapping_sub(1)]
                .into_iter()
                .filter(|&p| p < count),
        );
        pages
    }
}
