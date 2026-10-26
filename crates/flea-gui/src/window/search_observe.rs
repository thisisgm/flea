//! The query line's focusView and actual prepared-row paint observations; keyDeliveryState lives in window/key_delivery.rs.
use crate::{g2::sr1::Search, state::Shell};
pub(crate) struct QueryReader<'a> {
    pub search: &'a Search,
    pub focused: bool,
}

impl crate::ipc::Adapter for QueryReader<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<flea::ipc::wire::Reply> {
        (function == "focusView" && self.focused && self.search.typing())
            .then(|| flea::ipc::wire::Reply::value("search"))
    }
}

/// Metadata captured by the List's own paint, released after paint without reentering its owner.
pub(crate) struct Draw {
    pub side: usize,
    pub generation: usize,
    pub listing: usize,
    pub clock: crate::pane::clock::Clock,
    pub shell: gpui::WeakEntity<Shell>,
}

impl Draw {
    pub fn painted(&self, rows: &[crate::list::Shown], cx: &mut gpui::App) {
        if self.generation == 0 || !rows.iter().any(|row| row.prepared.is_some()) {
            return;
        }
        let (side, generation, listing) = (self.side, self.generation, self.listing);
        let at = self.clock.elapsed().as_millis() as u64;
        let shell = self.shell.clone();
        cx.defer(move |cx| {
            let _ = shell.update(cx, |shell, cx| {
                let Some(host) = shell.panes[side].as_mut() else {
                    return;
                };
                if host.pane.held.listing != listing || host.pane.swap.session.drops(generation) {
                    return;
                }
                let before = host.search.owner.session.stamps.first_rows_at;
                host.search.owner.rows_drawn(generation, at);
                if before != host.search.owner.session.stamps.first_rows_at {
                    cx.notify();
                }
            });
        });
    }
}
