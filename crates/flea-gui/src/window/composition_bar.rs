//! Settings > View "Window bar": the geometry its two choices beside "As shipped" lay out.
use super::composition::Geometry;

impl Geometry {
    /// A cover under "Window bar" Across the window: it takes the pane's whole height for its own strip, while the
    /// places keep their place below the band the bar would fill, so they never jump when the cover comes and goes.
    /// Its strip then spans the window from the left edge, as the bar does, and its rows keep the places' width as
    /// their inset; the places draw over the cover below that strip.
    pub fn cover_below_bar(&mut self) {
        self.chrome.height = 0.;
        self.tabs.height = 0.;
        let inset = if self.rail_overlay {
            0.
        } else {
            self.panes[0].x
        };
        self.panes[0].x -= inset;
        self.panes[0].width += inset;
        self.cover_inset = inset;
        self.panes[0].y = 0.;
        self.panes[0].height = self.status.y;
    }

    /// "Window bar" Over the list: the bar and the tabs start where the docked places end, and the places reach the
    /// top, as a cover such as Trash draws them. Hidden or overlaid places leave the bar across the window.
    pub fn bar_over_list(&mut self) {
        if self.rail_overlay || self.rail.width <= 0. {
            return;
        }
        let left = self.rail.x + self.rail.width;
        for bar in [&mut self.chrome, &mut self.tabs] {
            bar.width = (bar.x + bar.width - left).max(0.);
            bar.x = left;
        }
        self.rail.height += self.rail.y;
        self.rail.y = 0.;
    }
}
