//! Selection and gallery entry points retain the opener transaction owner.
use super::*;

impl Shell {
    pub(in crate::window::dispatch::opener) fn open_selection(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        // A mark on the cursor row alone is Enter's own open; any other mark set opens as the selection.
        if host.selected.is_empty() || host.selected.as_slice() == [host.pane.cursor] {
            return false;
        }
        if host.pane.busy() || host.stopped {
            return true;
        }
        if self.opener[side].pending.is_some() || self.opener[side].guard.pending() {
            let text = busy_text(&self.locale.catalog);
            self.opener_say(side, text, false, cx);
            return true;
        }
        let mut rows =
            flea::ui_logic_listing_filter::keep(&host.selected, host.filter.shown.as_deref());
        rows.sort_unstable();
        rows.dedup();
        if rows.is_empty() {
            return true;
        }
        if rows.len() > MAX_MARKED {
            let failed = failed_text(&self.locale.catalog);
            self.opener_say(side, failed, true, cx);
            return true;
        }
        // A loaded folder or archive refuses a batch up front; one marked row resolves first, so an archive reaches Quick Look.
        if let Some(refusal) = rows.iter().filter(|_| rows.len() > 1).find_map(|&index| {
            host.pane.held.row(index).and_then(|row| {
                if row.dir {
                    Some(OpenRefusal::FolderWithFiles)
                } else if quick_look_kind(&row.icon, &row.name, false) == State::Archive
                    && !row.name.to_ascii_lowercase().ends_with(".appimage")
                {
                    Some(OpenRefusal::Archives)
                } else {
                    None
                }
            })
        }) {
            self.opener_refused(side, refusal, cx);
            return true;
        }
        let listing = host.pane.held.listing;
        let path = host.pane.path().to_owned();
        let cursor = host.pane.cursor;
        self.menu.sequence += 1;
        let id = self.menu.sequence;
        let indices = rows
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.opener[side].pending = Some(Pending::Selection {
            id,
            listing,
            path,
            rows,
            kind: None,
        });
        self.opener_request(side, format!(r#"{{"c":"menuaction","op":"snapshot","id":{id},"listing":{listing},"rows":[{indices}],"cursor":{cursor}}}"#), cx);
        self.opener_watch(side, id, window, cx);
        true
    }

    // A resolution the backend never answers releases the lane instead of holding every later open.
    pub(super) fn opener_watch(
        &mut self,
        side: usize,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.opener[side].wait = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(RESOLVE_TIMEOUT).await;
            let _ = this.update(cx, |shell, cx| {
                if shell.opener[side].pending.as_ref().and_then(Pending::id) == Some(id) {
                    shell.open_selection_end(side, id, cx);
                    shell.opener_refused(side, OpenRefusal::NotLoaded, cx);
                }
            });
        }));
    }

    pub(in crate::window::dispatch::opener) fn open_menu_selection(
        &mut self,
        chosen: &Chosen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = chosen.captured.side;
        let current = self.panes[side].as_ref().is_some_and(|host| {
            !host.pane.busy()
                && !host.stopped
                && host.pane.held.listing == chosen.captured.listing
                && (chosen.captured.target.is_some() || host.pane.path() == chosen.captured.path)
        });
        if current && chosen.captured.ready && !chosen.paths.is_empty() {
            self.open_paths(side, chosen.paths.clone(), true, window, cx);
        }
    }

    /// The menu's Open on one validated path: an archive goes to Quick Look on its own row, any other file to flea --open.
    pub(in crate::window::dispatch::opener) fn open_menu_target(
        &mut self,
        chosen: &Chosen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = chosen.captured.side;
        let idle = self.panes[side]
            .as_ref()
            .is_some_and(|host| !host.stopped && !host.pane.busy());
        let [target] = chosen.paths.as_slice() else {
            return;
        };
        if !idle || !chosen.captured.ready {
            return;
        }
        // The cursor's own file keeps Enter's path, its image gallery included.
        if let Some((path, archive)) = file_row(self, side).filter(|(path, _)| path == target) {
            self.open_cursor_file(side, path, archive, window, cx);
            return;
        }
        let action = chosen.captured.target.as_ref().and_then(|row| {
            if archive_path(target) {
                Some("preview")
            } else if quick_look_kind(&row.icon, &row.name, false) == State::Image {
                Some("open")
            } else {
                None
            }
        });
        if let Some(action) = action {
            self.navigate_menu_action(
                side,
                chosen.captured.path.clone(),
                action,
                Some(target.clone()),
                window,
                cx,
            );
            return;
        }
        match chosen.captured.rows.first() {
            Some(&index) if chosen.captured.target.is_none() && archive_path(target) => {
                self.open_archive_path(side, index, target.clone(), window, cx)
            }
            _ => self.open_file(side, target.clone(), window, cx),
        }
    }

    pub(in crate::window::dispatch::opener) fn open_gallery(
        &mut self,
        side: usize,
        first: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        let Some(row) = host.pane.held.row(host.pane.cursor) else {
            return false;
        };
        if quick_look_kind(&row.icon, &row.name, false) != State::Image {
            return false;
        }
        if self.opener[side].pending.is_some()
            || self.opener[side].guard.pending()
            || host.nav.gate.paths_pending
            || host.nav.gate.clip_pending
            || self.rename.paths == Some(side)
            || self.carry_pending(side, window, cx)
        {
            let text = busy_text(&self.locale.catalog);
            self.opener_say(side, text, false, cx);
            return true;
        }
        let ask = viewerorder::ask(
            host.pane.cursor,
            host.pane.held.listing as u64,
            &host.selected,
            host.filter.shown.as_deref(),
        );
        let held = viewerorder::Held {
            listing: ask.listing,
            path: host.pane.path().to_owned(),
            in_flight: false,
        };
        let sent = host
            .backend
            .as_ref()
            .is_some_and(|backend| backend.line(ask.request()).is_ok());
        if sent {
            self.panes[side]
                .as_mut()
                .expect("the gallery pane")
                .nav
                .gate
                .paths_pending = true;
            self.opener[side].pending = Some(Pending::Gallery { ask, held, first });
        } else {
            self.opener_refused(side, OpenRefusal::NotLoaded, cx);
        }
        true
    }
}

impl Pending {
    pub(super) fn id(&self) -> Option<u64> {
        match self {
            Self::Selection { id, .. } | Self::Archive { id, .. } => Some(*id),
            Self::Gallery { .. } => None,
        }
    }
}
