//! A symlink target reply belongs to one request, pane and listing, so a late answer cannot navigate another view.
use crate::{backend::Event, state::Shell};
use flea::{
    json::escape,
    jsondoc::{self, Json},
};
use gpui::{Context, Window};

#[derive(Default)]
pub(crate) struct Originals {
    sequence: u64,
    pending: [Option<Pending>; 2],
}

struct Pending {
    id: u64,
    link: String,
    origin: String,
    listing: usize,
}

impl Shell {
    pub(crate) fn menu_original_start(
        &mut self,
        side: usize,
        link: String,
        _window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes.get(side).and_then(Option::as_ref) else {
            return;
        };
        let Some(id) = self.menu.originals.sequence.checked_add(1) else {
            return;
        };
        self.menu.originals.sequence = id;
        self.menu.originals.pending[side] = Some(Pending {
            id,
            link: link.clone(),
            origin: host.pane.place().to_owned(),
            listing: host.pane.held.listing,
        });
        self.menu_send(
            side,
            format!(
                r#"{{"c":"linktarget","path":"{}","id":{id}}}"#,
                escape(&link)
            ),
            cx,
        );
    }

    pub(crate) fn menu_original_event(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if side >= self.menu.originals.pending.len() {
            return false;
        }
        if matches!(event, Event::Failed(_) | Event::Drained) {
            self.menu.originals.pending[side] = None;
            return false;
        }
        if let Event::Refused {
            operation, path, ..
        } = event
        {
            if operation == "linktarget"
                && self.menu.originals.pending[side]
                    .as_ref()
                    .is_some_and(|pending| &pending.link == path)
            {
                self.menu.originals.pending[side] = None;
            }
            return false;
        }
        let Event::Producer(line) = event else {
            return false;
        };
        let Ok(reply) = jsondoc::parse(line) else {
            return false;
        };
        let text = |key| reply.get(key).and_then(Json::as_str).unwrap_or("");
        if text("t") != "linktarget" {
            return false;
        }
        let id = reply.get("id").and_then(|value| match value {
            Json::Num(value) => value.parse::<u64>().ok(),
            _ => None,
        });
        let pending = &mut self.menu.originals.pending[side];
        if !pending
            .as_ref()
            .is_some_and(|pending| Some(pending.id) == id && pending.link == text("path"))
        {
            return true;
        }
        let pending = pending.take().expect("matched request");
        if self.panes[side].as_ref().is_none_or(|host| {
            host.stopped
                || host.pane.busy()
                || host.pane.place() != pending.origin
                || host.pane.held.listing != pending.listing
        }) {
            return true;
        }
        let (directory, name) = (text("directory"), text("name"));
        if directory.is_empty() || (name.is_empty() && directory != "/") {
            self.menu_notice(side, self.locale.catalog.tr("menu.original.missing"), cx);
        } else {
            super::menu_parity::reveal(self, side, directory, name, window, cx);
        }
        true
    }
}
