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

impl Shell {
    pub(super) fn filesystem_flags(&mut self, side: usize, event: Event) -> Event {
        let (system, event) = match event {
            Event::FilesystemSystem { system, reply } => (system, *reply),
            event => (false, event),
        };
        self.quick_prepare_storage(side, &event);
        let path = match &event {
            Event::Filesystem(facts) | Event::FilesystemDetails { facts, .. } => &facts.path,
            _ => return event,
        };
        if let Some(host) = &self.panes[side] {
            if path.is_empty() || path == &host.path {
                let filesystem = &mut self.surfaces[side].filesystem;
                filesystem.enter(&host.path);
                filesystem.system = system;
            }
        }
        event
    }

    pub(super) fn filesystem_answer(
        &mut self,
        side: usize,
        facts: crate::backend::status::Filesystem,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = &self.panes[side] else {
            return;
        };
        if self.surfaces[side].filesystem.answer(&host.path, facts) {
            cx.notify();
        }
    }
}
