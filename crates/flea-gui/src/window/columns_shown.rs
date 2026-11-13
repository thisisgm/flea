//! The Columns search strip height and the view a frame draws while an entry into Columns waits for its columns.
use crate::{state::Shell, window::startup::View};

impl crate::window::pane::PaneHost {
    /// An entry into Columns held on the List keeps the List's rows height, so its asks and the held rows share one window size.
    pub(crate) fn landing_height(&self, view: View, list: f32, full: f32) -> f32 {
        if view == View::Columns
            && self
                .columns
                .as_ref()
                .is_none_or(|columns| columns.entering || columns.arm_entry)
        {
            list
        } else {
            full
        }
    }
}

impl Shell {
    /// The search strip's height over a Columns pane, which draws it in the header slot List leaves it.
    pub(crate) fn columns_search_strip(&self, side: usize) -> f32 {
        self.panes[side]
            .as_ref()
            .filter(|host| self.view == View::Columns && host.search.owner.active())
            .map_or(0., |host| {
                crate::list::Metrics::for_typeface(&host.list.borrow().typeface).header
            })
    }

    /// The view whose content this frame draws: an entry into Columns still waiting on its columns draws the List.
    pub(crate) fn shown_view(&self) -> View {
        let entering = self.panes[0]
            .as_ref()
            .and_then(|host| host.columns.as_ref())
            .is_some_and(|columns| columns.entering);
        if self.view == View::Columns && entering {
            View::List
        } else {
            self.view
        }
    }
}
