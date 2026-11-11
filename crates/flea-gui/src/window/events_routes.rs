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
            Some("unmounted") => {
                self.left_folder(side, &doc, "dialogs.file.disconnected", window, cx)
            }
            Some("gone") => self.left_folder(side, &doc, "tabs.folder_gone", window, cx),
            _ => return false,
        }
        true
    }

    // The open folder's volume went, or the folder or one above it was deleted or moved: the pane and every hidden tab inside it land on the nearest folder still there.
    fn left_folder(
        &mut self,
        side: usize,
        doc: &Json,
        said: &'static str,
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
        let root = crate::tabs::gone::vanished_root(path, parent);
        if let Some(root) = &root {
            let mut moved = false;
            for tabs in &mut self.strip.tabs {
                moved |= tabs.land_gone(root, parent);
            }
            if moved {
                cx.notify();
            }
        }
        // A list still out inside the folder that went, an ancestor of path included, would fail too, so it gives way to parent.
        let scope = root.as_deref().unwrap_or(path);
        let affected = self.panes[side].as_ref().is_some_and(|host| {
            let place = host.pane.place();
            place == scope
                || place
                    .strip_prefix(scope)
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
        let said = self.locale.catalog.tr(said);
        self.opener_say(side, said, false, cx);
    }
}
