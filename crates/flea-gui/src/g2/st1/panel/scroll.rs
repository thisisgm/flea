//! The pane's scroll under the wheel, over a section's rows or a standing search's results.
use super::{geometry, Panel};

impl Panel {
    /// A wheel turn's travel down the list the pane shows, kept inside the content.
    pub fn wheel(&mut self, travel: f32) {
        if self.query.as_deref().is_some_and(|query| !query.is_empty()) {
            let most = (self.results_height - self.pane.1).max(0.);
            self.results_scroll = (self.results_scroll + travel).clamp(0., most);
            return;
        }
        let most = (self.heights().iter().sum::<f32>() - self.pane.1).max(0.);
        self.scroll = (self.scroll + travel).clamp(0., most);
    }

    /// The drawn results' heights and the lines the highlighted one shows on; returns the scroll they are drawn at.
    pub fn results_laid_out(&mut self, heights: &[f32], hit: Option<(usize, usize)>) -> f32 {
        self.results_height = heights.iter().sum();
        let viewport = self.pane.1;
        if std::mem::take(&mut self.hit_moved) {
            if let Some((from, at)) = hit.filter(|(from, at)| from <= at && *at < heights.len()) {
                let top: f32 = heights[..from].iter().sum();
                let bottom = heights[..=at].iter().sum::<f32>() + geometry::FADE;
                if top < self.results_scroll {
                    self.results_scroll = top;
                } else if bottom > self.results_scroll + viewport {
                    self.results_scroll = bottom - viewport;
                }
            }
        }
        self.results_scroll = self
            .results_scroll
            .clamp(0., (self.results_height - viewport).max(0.));
        self.results_scroll
    }
}
