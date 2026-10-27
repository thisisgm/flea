//! The hold that keeps the first screen until its shown blocks are laid and what they draw has arrived or run out of time.
use super::super::{MarkdownView, Status, View};
use super::{picture_urls, PictureState};
use gpui::Context;
use std::time::Duration;

/// The longest the first screen waits on a picture, figure or formula before it draws without it.
pub const FIGURE_HOLD_MS: u64 = 1500;

impl MarkdownView {
    /// True while the first screen waits: the shown blocks not yet laid at this width, or a picture, figure or colour they draw still out.
    pub fn holding(&self, now: u64) -> bool {
        if self.confirming {
            return true;
        }
        if self.doc.shown {
            return false;
        }
        if self.shown_view() == View::Source {
            return self.source_refused().is_none() && !self.source.first_screen();
        }
        if self.doc.status != Status::Ready && self.doc.blocks().is_empty() {
            return matches!(self.doc.status, Status::Loading);
        }
        let figures_late = now.saturating_sub(self.doc.loaded_at) >= FIGURE_HOLD_MS;
        self.visible().any(|i| {
            if !self.doc.current(i) {
                return true;
            }
            let mut urls = Vec::new();
            if let Some(laid) = &self.doc.laid[i] {
                picture_urls(&laid.geometry.items, &mut urls);
            }
            let picture_out = urls
                .iter()
                .any(|url| matches!(self.doc.pictures.get(url), Some(PictureState::Asked) | None));
            let figure_out = self
                .doc
                .ordinals(i, |c| c.figure)
                .any(|o| !self.doc.assets.figures.contains_key(&o));
            let formula_out = self
                .doc
                .ordinals(i, |c| c.formula)
                .any(|o| !self.doc.formulas_settled.contains(&o));
            let fence_out = self.code.is_some()
                && self.doc.ordinals(i, |c| c.fence).any(|o| {
                    let named = self
                        .doc
                        .parsed
                        .asks
                        .fences
                        .get(o)
                        .is_some_and(|(_, info)| !info.trim().is_empty());
                    named && !self.doc.fences.get(&o).is_some_and(|f| f.done)
                });
            fence_out || (!figures_late && (picture_out || figure_out || formula_out))
        })
    }

    /// Ends the first-screen hold once nothing it waits on is out, and wakes again when its figure wait runs out; with no document there is nothing to release.
    pub(in crate::g2::pv2::markdown) fn release_hold(&mut self, cx: &mut Context<Self>) {
        if self.doc.shown || self.frame.1 <= 0. || self.doc.status == Status::Idle {
            return;
        }
        let now = self.now(cx);
        if !self.holding(now) {
            self.doc.shown = true;
            cx.notify();
            return;
        }
        let due = self.doc.loaded_at + FIGURE_HOLD_MS;
        if now < due && self.hold_timer.is_none() {
            let (wait, seq) = (due - now, self.doc.seq);
            self.hold_timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(wait))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.hold_timer = None;
                    if view.doc.seq == seq {
                        view.release_hold(cx);
                    }
                });
            }));
        }
    }
}
