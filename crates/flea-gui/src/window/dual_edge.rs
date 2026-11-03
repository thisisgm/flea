//! The focused Dual pane keeps the shipped accent edge below its path strip.
use super::{composition::Geometry, startup::View};
use crate::{
    list::{Metrics, Palette},
    state::Shell,
};
use gpui::{div, px, Div, Styled};

impl Shell {
    pub(super) fn dual_edge(&self, geometry: &Geometry, palette: Palette) -> Option<Div> {
        if self.view != View::Dual
            || self.panes.get(self.focus_side)?.is_none()
            || self.slots.covers.get(self.focus_side)?.is_some()
        {
            return None;
        }
        let pane = geometry.panes.get(self.focus_side)?;
        let header = Metrics::for_typeface(&self.active_typeface()).header;
        let height = (pane.height - header).max(0.);
        (height > 0. && pane.width > 0.).then(|| {
            div()
                .absolute()
                .left(px(pane.x))
                .top(px(pane.y + header))
                .w(px(pane.width.min(2.)))
                .h(px(height))
                .bg(palette.accent_frame)
        })
    }
}
