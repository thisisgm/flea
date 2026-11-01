//! Tag replies update the captured selection and explain completed writes through the existing status service.
use super::{
    delivery::string,
    tags::{entries, Asked},
};
use crate::{backend::Event, state::Shell};
use flea::{
    jsondoc::{self, Json},
    ui_logic_pl4::tags,
};
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn tags_event(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match event {
            Event::Failed(_) | Event::Drained => {
                self.menu.tags.pending[side].clear();
                self.menu.tags.refresh[side] = None;
                if self
                    .menu
                    .tags
                    .asking
                    .as_ref()
                    .is_some_and(|ask| ask.side == side)
                {
                    self.menu.tags.asking = None;
                }
            }
            Event::Refused {
                operation, path, ..
            } if operation == "settags" || (operation == "stale" && path == "settags") => {
                self.menu.tags.pending[side].pop_front();
            }
            Event::Producer(line) | Event::Operations(line) => {
                let Ok(reply) = jsondoc::parse(line) else {
                    return false;
                };
                if string(&reply, "t") == "tagstates" {
                    self.tag_states_done(side, &reply, window, cx);
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn tag_states_done(
        &mut self,
        side: usize,
        reply: &Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Asked {
            id,
            menu,
            listing,
            identity,
            ..
        }) = self
            .menu
            .tags
            .asking
            .as_ref()
            .filter(|ask| ask.side == side)
        else {
            return;
        };
        if number(reply, "id") != *id || number(reply, "listing") != *listing as u64 {
            return;
        }
        if self
            .menu
            .captured
            .as_ref()
            .is_none_or(|capture| capture.id != *menu || capture.identity != *identity)
            || self.menu_identity() != *identity
        {
            return;
        }
        self.menu.tags.asking = None;
        let flyout = std::mem::take(&mut self.menu.tags_reading);
        let names = |key| {
            reply
                .get(key)
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(Json::as_str)
                .collect::<Vec<_>>()
        };
        self.menu.context.tag_submenu = Some(entries(&tags::states_from(
            &names("applied"),
            &names("mixed"),
        )));
        if self.menu_opened() {
            let rows = self.menu_entries();
            let list = self.panes[side].as_ref().unwrap().list.borrow();
            self.menu
                .menu
                .borrow_mut()
                .refresh(rows, &self.menu_env(&list.typeface), window);
        } else {
            self.refresh_menu(window, cx);
        }
        if flyout {
            self.flyout_pending(window, cx);
        }
        cx.notify();
    }

    pub(super) fn tag_write_done(&mut self, side: usize, reply: &Json, cx: &mut Context<Self>) {
        let Some(change) = self.menu.tags.pending[side].pop_front() else {
            return;
        };
        let name = if change.tag.is_empty() {
            String::new()
        } else {
            self.locale.catalog.tr(tags::label_key(change.tag))
        };
        let skipped = reply
            .get("skipped")
            .and_then(Json::as_array)
            .map_or(0, |rows| rows.len());
        let said = tags::done_line(
            &self.locale.catalog,
            self.tag_preset(),
            tags::Op::from_wire(string(reply, "op")),
            number(reply, "ok") as i64,
            &name,
            skipped,
        );
        self.menu.tags.refresh[side] = Some(change.listing);
        if let Some(host) = self.panes[side].as_mut() {
            if !said.is_empty() {
                host.status = said.clone().into();
            }
        }
        if !said.is_empty() {
            let (said, hint) = self.split_undo(&said, false);
            self.status_messages(cx, |messages, now| {
                messages.say(&said, false, "", hint, now)
            });
        }
        self.ask_tag_states(true, cx);
        self.refresh_tag_window(side, cx);
        cx.notify();
    }

    pub(super) fn refresh_tag_window(&mut self, side: usize, cx: &mut Context<Self>) {
        let Some(listing) = self.menu.tags.refresh[side] else {
            return;
        };
        let Some(host) = self.panes[side].as_mut() else {
            self.menu.tags.refresh[side] = None;
            return;
        };
        if host.pane.busy() || host.pane.held.listing != listing || host.stopped {
            self.menu.tags.refresh[side] = None;
            return;
        }
        let Some(backend) = &host.backend else {
            self.menu.tags.refresh[side] = None;
            return;
        };
        let Some(request) = host.pane.reread_held() else {
            return;
        };
        self.menu.tags.refresh[side] = None;
        if let Err(error) = backend.window(request.start, request.count) {
            host.pane.requests.cancel();
            host.record_error(error, &self.locale);
            cx.notify();
        }
    }
}

fn number(reply: &Json, key: &str) -> u64 {
    match reply.get(key) {
        Some(Json::Num(value)) => value.parse().unwrap_or(0),
        _ => 0,
    }
}
