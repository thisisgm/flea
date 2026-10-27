//! Each preview, provider or clipboard producer can take its own reply before listing dispatch.
use crate::{backend::Event, state::Shell};
use gpui::{Context, Window};

impl Shell {
    pub(super) fn producer_reply(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.quick_look_line(side, event, window, cx)
            || self.selection_preview_event(side, event, window, cx)
            || self.grid_pictures_event(side, event, window, cx)
            || self.provider_event(side, event, window, cx)
            || self.clipboard_receive(side, event, window, cx)
    }
}
