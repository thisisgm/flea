//! The Columns loading marks: the crawl each loading track shows, and its paint in the window's own frame.
use crate::{columns::TrackState, state::Shell};
use std::rc::Rc;

impl Shell {
    pub(crate) fn column_loading_marks(
        &self,
        window: &gpui::Window,
        cx: &gpui::App,
    ) -> [Option<Rc<crate::feedback::spinner::Crawl>>; 5] {
        let mut marks = std::array::from_fn(|_| None);
        if self.shown_view() != super::super::startup::View::Columns
            || self.slots.covers[0].is_some()
        {
            return marks;
        }
        let Some(columns) = self.panes[0]
            .as_ref()
            .and_then(|host| host.columns.as_ref())
        else {
            return marks;
        };
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        for track in columns
            .area
            .tracks()
            .iter()
            .filter(|track| track.state == TrackState::Loading)
        {
            if let Some(mark) = &columns.loading[track.slot.index()] {
                mark.show(true, palette.accent);
                marks[track.slot.index()] = Some(Rc::clone(mark));
            }
        }
        marks
    }

    pub(crate) fn paint_column_loading(&self, window: &mut gpui::Window, _cx: &gpui::App) {
        if self.shown_view() != super::super::startup::View::Columns
            || self.slots.covers[0].is_some()
        {
            return;
        }
        let Some(columns) = self.panes[0]
            .as_ref()
            .and_then(|host| host.columns.as_ref())
        else {
            return;
        };
        for track in columns
            .area
            .tracks()
            .iter()
            .filter(|track| track.state == TrackState::Loading)
        {
            if let Some(mark) = &columns.loading[track.slot.index()] {
                mark.paint(window);
            }
        }
    }
}
