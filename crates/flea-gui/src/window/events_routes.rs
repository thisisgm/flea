//! Route non-listing replies before the pane consumes a listing message.
use crate::{backend::Event, state::Shell};
use flea::jsondoc::{self, Json};
use gpui::{Context, Window};

impl Shell {
    pub(super) fn route_surface_event(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.opener_wire(side, event, window, cx)
            || self.quick_look_line(side, event, window, cx)
            || self.selection_preview_event(side, event, window, cx)
            || self.grid_pictures_event(side, event, window, cx)
            || self.provider_event(side, event, window, cx)
            || self.clipboard_receive(side, event, window, cx)
            || self.nearby_event(side, event, window, cx)
    }

    // A Trash line goes to the operation view first when one is open, then to the Trash pane.
    pub(super) fn trash_event(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Event::TrashLine(line) = event else {
            return false;
        };
        if self.operation_view(side).is_some() {
            self.operation_line(side, line, window, cx);
        }
        self.trash_line(side, line, cx);
        true
    }

    fn opener_wire(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Event::Producer(line) = event else {
            return false;
        };
        let Ok(doc) = jsondoc::parse(line) else {
            return false;
        };
        match doc.get("t").and_then(Json::as_str) {
            Some("linktarget") => self.original_reply(side, &doc, window, cx),
            Some("unmounted") => self.disconnected(side, &doc, window, cx),
            _ => return false,
        }
        true
    }

    fn disconnected(
        &mut self,
        side: usize,
        doc: &Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = |key| doc.get(key).and_then(Json::as_str);
        let (Some(path), Some(parent)) = (text("path"), text("parent")) else {
            return;
        };
        if !path.starts_with('/') || !parent.starts_with('/') || parent.is_empty() {
            return;
        }
        let affected = self.panes[side].as_ref().is_some_and(|host| {
            let place = host.pane.place();
            place == path
                || place
                    .strip_prefix(path)
                    .is_some_and(|tail| tail.starts_with('/'))
        });
        if !affected {
            return;
        }
        self.search_leave(side, window, cx);
        if self
            .menu
            .captured
            .as_ref()
            .is_some_and(|capture| capture.side == side)
        {
            self.retire_menu(window, cx);
        }
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        host.nav.pending_select = None;
        let options = crate::pane::swap::Options {
            supersede: true,
            ..Default::default()
        };
        let _ = host
            .nav
            .open_without_history(&mut host.pane, parent, options, None, false);
        self.run_navigation(side, window, cx);
        let said = self.locale.catalog.tr("dialogs.file.disconnected");
        self.opener_say(side, said, false, cx);
    }
}
