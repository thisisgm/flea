//! The production pane owns SR1; its effects join the existing navigation and backend queues.
use super::startup::View;
use crate::{
    g2::file_search::{Host, HostPane, Search, StripKey},
    pane::swap::Options,
    state::Shell,
};
use flea::ui_logic_file_search::{Kind, Scope};
use gpui::{Context, KeyDownEvent, Window};

#[path = "search_header.rs"]
mod header;

#[path = "search_empty.rs"]
mod empty;
#[path = "search_keys.rs"]
mod keys;
#[path = "search_observe.rs"]
mod observe;
pub(crate) use observe::{Draw, QueryReader};
#[cfg(test)]
#[path = "search_contents_tests.rs"]
mod contents_tests;
#[cfg(test)]
#[path = "enter_once_tests.rs"]
mod enter_once_tests;
#[cfg(test)]
#[path = "search_grid_tests.rs"]
mod grid_tests;
#[cfg(test)]
#[path = "../g2/file_search/i50_query_host_tests.rs"]
mod i50_query_host_tests;
#[cfg(test)]
#[path = "search_keep_tests.rs"]
mod keep_tests;
#[cfg(test)]
#[path = "search_presented_tests.rs"]
mod presented_tests;
#[cfg(test)]
#[path = "search_root_tests.rs"]
mod root_tests;
#[cfg(test)]
#[path = "search_scope_tests.rs"]
mod scope_tests;
#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;

#[path = "search_contents.rs"]
mod contents;
#[path = "search_host.rs"]
mod host;
#[path = "search_keep.rs"]
mod keep;
#[path = "search_lines.rs"]
mod lines;
pub(crate) use keep::Kept;
#[path = "search_tag.rs"]
mod tag;
pub(crate) use host::Runtime;
use host::{Effect, Pending};
impl Shell {
    pub(crate) fn search_update(
        &mut self,
        side: usize,
        clipboard: Option<String>,
        change: impl FnOnce(&mut Search, &mut dyn Host),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        let path = host.path.clone();
        let home = std::env::var("HOME").unwrap_or_default();
        let cursor = host
            .pane
            .held
            .row(host.pane.cursor)
            .map(|row| row.name.clone());
        let mut pending = Pending {
            pane: HostPane {
                path: &path,
                home: &home,
                hidden: host.nav.folders.borrow().hidden,
                count: host.pane.first_count(),
                listing: host.pane.held.listing,
                sizes: 0,
                cursor_name: cursor.as_deref(),
                recent: None,
                now: host.pane.now(),
            },
            clipboard,
            effects: Vec::new(),
            session: None,
        };
        let was = (host.search.owner.active(), host.search.owner.session.kind);
        change(&mut host.search.owner, &mut pending);
        if let Some(session) = pending.session {
            if session.drops(host.search.rows_generation) {
                host.search.kept = None;
            }
            host.pane.swap.session = session;
        }
        let mut relisted = false;
        let mut send_failed = false;
        for effect in pending.effects {
            match effect {
                Effect::Send(line) => {
                    if let Some(backend) = &host.backend {
                        if let Err(error) = backend.line(line) {
                            host.stopped = true;
                            send_failed = true;
                            host.record_error(error, &self.locale);
                        }
                    }
                }
                Effect::Relist(path, select, relist) => {
                    host.select = (!select.is_empty()).then_some(select);
                    if !relist {
                        host.nav.pending_select = host.select.clone();
                    }
                    // A search's exit under Recent lists the history again, and a reveal leaves Recent for its folder.
                    if host.recent.on && relist {
                        let _ = host
                            .nav
                            .list_paths(&mut host.pane, &host.recent.paths, true);
                    } else {
                        host.leave_recent();
                        let _ = host.nav.open_without_history(
                            &mut host.pane,
                            &path,
                            Options {
                                supersede: true,
                                hold_walk: true,
                                relist,
                                ..Options::default()
                            },
                            None,
                            false,
                        );
                    }
                    relisted = true;
                }
                Effect::Reset => {
                    host.pane.swap.rows_at = 0;
                }
                Effect::Settle => {}
            }
        }
        let active = host.search.owner.active();
        host.nav.gate.search = active;
        let kind = host.search.owner.session.kind;
        // A contents result is a List block (HIG search-contents); a names search keeps Grid's tiles.
        if active && was != (true, kind) {
            match kind {
                Kind::Contents if self.view == View::Grid => self.view = View::List,
                Kind::Names if host.search.origin == Some(View::Grid) => self.view = View::Grid,
                _ => {}
            }
        }
        if !active {
            if let Some(view) = host.search.origin.take() {
                host.pane.scroll_y = host.search.scroll;
                host.pane.scroll_by(0.);
                if self.view == View::List {
                    self.view = view;
                }
            }
        }
        if send_failed {
            self.search_landed(side, true, window, cx);
            return;
        }
        if relisted {
            self.run_navigation(side, window, cx);
        }
        self.arm_transition(side, window, cx);
        cx.notify();
    }

    pub(crate) fn search_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.input_blocked(cx) {
            return false;
        }
        let side = self.focus_side;
        let Some(host) = self.panes[side].as_mut() else {
            return false;
        };
        // Ctrl+G opens like f, already in contents; it does nothing until the contents producer is wired.
        let opens = action == "search"
            || (action == "searchContents" && host.search.owner.session.wire.contents);
        if host.stopped
            || host.backend.is_none()
            || (!opens && !(action == "reveal" && host.search.owner.active()))
        {
            return false;
        }
        // A raw filename the producer could not name exactly is never revealed from its display text.
        if action == "reveal"
            && !host
                .search
                .owner
                .operable(host.pane.held.listing, host.pane.cursor)
        {
            return true;
        }
        if opens && !host.search.owner.active() {
            if host.pane.busy() {
                return true;
            }
            // Each fresh open starts here; Home and wider scopes belong to the current search.
            host.search.owner.session.scope = Scope::Folder;
            // Every view keeps itself: Columns per HIG cols4-hero-search, Grid's tiles per v0.3.12 GridArea.
            host.search.origin = Some(self.view);
            host.search.scroll = host.pane.scroll_y;
        }
        self.search_update(
            side,
            None,
            |search, host| {
                search.action(action, host);
            },
            window,
            cx,
        );
        window.focus(&self.focus, cx);
        true
    }

    pub(crate) fn search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.focus.is_focused(window) {
            return false;
        }
        let side = self.focus_side;
        let Some(host) = &self.panes[side] else {
            return false;
        };
        if !host.search.owner.active() {
            return false;
        }
        let key = StripKey::from_event(event);
        // Enter on a result is the listing's: a file opens in its default app and a folder goes in; o reveals.
        if !host.search.owner.typing() && !matches!(key, StripKey::Escape) {
            return false;
        }
        // Focus.js leavesLine lets non-printable cursor keys reach the listing while the query keeps typing.
        let name = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default");
        if host.search.owner.typing()
            && host.search.owner.entry.borrow().marked.is_none()
            && keys::leaves_line(name, event, window.modifiers())
        {
            return host.search.owner.session.leaving;
        }
        let clipboard = matches!(key, StripKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        // Enter on the line commits and acts on the highlighted result in the same press; rows still out act when they land.
        let enter = host.search.owner.typing()
            && matches!(key, StripKey::Enter)
            && crate::window::input::modifiers(event.keystroke.modifiers) == 0
            && host.search.owner.entry.borrow().marked.is_none();
        self.search_update(
            side,
            clipboard,
            |search, host| search.key_event(event, host),
            window,
            cx,
        );
        if enter {
            self.search_enter_row(side, event, window, cx);
        }
        true
    }

    /// The newest generation's rows landed: an Enter that came before them acts on the highlighted result now.
    pub(crate) fn search_rows_landed(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let Some(generation) = host.search.enter_after else {
            return;
        };
        if host
            .search_scene()
            .is_some_and(|(drawn, _)| drawn == generation)
        {
            host.search.enter_after = None;
            if side == self.focus_side && self.enter_replay_open(window, cx) {
                self.search_enter_row(side, &crate::window::filter::plain_enter(), window, cx);
            }
        }
    }

    // The committed line's Enter, run again through the listing so a file opens in its default app and a folder goes in.
    fn search_enter_row(
        &mut self,
        side: usize,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let owner = &host.search.owner;
        if !owner.active()
            || owner.typing()
            || owner.session.leaving
            || owner.session.query.is_empty()
        {
            return;
        }
        if host.search_scene().is_none() {
            host.search.enter_after = Some(owner.session.stamps.gen);
            return;
        }
        if host.pane.held.row(host.pane.cursor).is_none() {
            return;
        }
        self.key(event, window, cx);
    }

    pub(crate) fn search_leave(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.panes[side]
            .as_ref()
            .is_some_and(|host| host.search.owner.active() && !host.search.owner.session.leaving)
        {
            return;
        }
        self.search_update(side, None, |search, host| search.leave(host), window, cx);
    }

    pub(crate) fn sync_search_text(&mut self) {
        for host in self.panes.iter_mut().flatten() {
            if host.search.owner.session.live() {
                let (status, _) = Search::status(&host.search.owner, &self.locale.catalog);
                host.state_text.message = status.clone();
                host.status = status.into();
            }
        }
    }
}
