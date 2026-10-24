//! Deliver the two owned projections of a single backend filesystem reply.
use crate::{backend::Event, state::Shell};
use gpui::Context;

impl Shell {
    pub(super) fn filesystem_details(
        &mut self,
        side: usize,
        event: Event,
        cx: &mut Context<Self>,
    ) -> Event {
        match event {
            Event::FilesystemDetails { facts, class } => {
                self.list_details(
                    side,
                    crate::list::sizes::Reply::Filesystem {
                        path: facts.path.clone(),
                        class,
                    },
                    cx,
                );
                Event::Filesystem(facts)
            }
            event => event,
        }
    }
}
