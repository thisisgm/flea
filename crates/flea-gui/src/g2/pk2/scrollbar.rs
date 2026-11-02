//! PickerList and PickerGrid's ViewportScrollBar: the chooser body shares the document views' lane, knob and fade (EY-377).
use super::pictures::RequestLine;
use super::view::ListingView;
use crate::g2::pv1::scroller::{
    overlay::{Overlay, Owner},
    View,
};
use crate::grid::Shown;
use gpui::{Bounds, Context, Pixels};

impl ListingView {
    // The rows the body draws: the name filter's when one stands, the whole listing otherwise.
    fn drawn_total(&self) -> usize {
        self.shown().map_or(self.listing().total(), <[usize]>::len)
    }

    #[cfg(test)]
    pub fn bar_shown(&self) -> bool {
        self.bars.vertical_shown()
    }
}

impl Owner for ListingView {
    fn overlay(&mut self) -> &mut Overlay {
        &mut self.bars
    }

    fn viewport(&self, bounds: Bounds<Pixels>) -> Option<(View, (f64, f64))> {
        let content = if self.surface.mode == "list" {
            self.drawn_total() as f32 * self.state.borrow().file_metrics().row
        } else {
            self.grid_view()
                .layout(Shown::All(self.drawn_total()))
                .extent()
        };
        Some((
            View {
                bounds,
                content_w: f64::from(f32::from(bounds.size.width)),
                content_h: f64::from(content),
                sideways: false,
            },
            (0., f64::from(self.scroll_y)),
        ))
    }

    // A knob drag or lane press moves the body and asks for the rows it now shows, as the wheel does.
    fn position(&mut self, position: (f64, f64), cx: &mut Context<Self>) {
        let line = if self.surface.mode == "list" {
            self.set_scroll_y(position.1 as f32);
            self.list_window_request()
        } else {
            let layout = self.grid_view().layout(Shown::All(self.drawn_total()));
            self.set_scroll_y(layout.clamp(position.1 as f32));
            self.grid_window_request()
        };
        if let Some(line) = line {
            cx.emit(RequestLine(line));
        }
        cx.notify();
    }
}
