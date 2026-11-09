//! The active pane's Recent: the history listed as paths, the way back out, and the keys that leave it.
use super::{navigate::Step, pane::PaneHost};
use crate::{
    navigation::{folder_state::Order, Refusal},
    pane::swap::Options,
    state::Shell,
};
use flea::ui_logic_rail_places::{
    recent::{self, Visit},
    recentmode::RecentMode,
};
use gpui::{Context, SharedString, Window};

/// The path bar's text: the history's own name while Recent stands, the folder otherwise.
pub(crate) fn path_label(host: &PaneHost, recent_name: &str) -> SharedString {
    if host.recent.on {
        recent_name.into()
    } else {
        host.path.clone().into()
    }
}

impl PaneHost {
    /// Leaves Recent and hands back the folder order it replaced; true when Recent was standing.
    pub(crate) fn leave_recent(&mut self) -> bool {
        let was = self.recent.on;
        if let Some((key, desc)) = self.recent.stashed.take() {
            self.nav.order = Order {
                key: sort_key(&key),
                desc,
            };
        }
        self.recent = RecentMode::default();
        self.nav.leave_paths();
        if was {
            self.list.borrow_mut().set_recent_place(None);
        }
        was
    }

    /// Back, Escape and Up: Recent closes onto the folder it was opened over, with no history entry.
    pub(crate) fn close_recent(&mut self) -> Result<bool, Refusal> {
        let path = if self.recent.from.is_empty() {
            "/".into()
        } else {
            self.recent.from.clone()
        };
        self.leave_recent();
        let preserve = std::mem::replace(&mut self.nav.preserve_sort, true);
        let result = self.nav.open_without_history(
            &mut self.pane,
            &path,
            Options {
                supersede: true,
                ..Options::default()
            },
            None,
            false,
        );
        self.nav.preserve_sort = preserve;
        result
    }
}

fn sort_key(key: &str) -> &'static str {
    match key {
        "size" => "size",
        "mtime" => "mtime",
        "kind" => "kind",
        "mode" => "mode",
        _ => "name",
    }
}

impl Shell {
    /// The history read landed for the side that asked; a navigation since then left no ticket to answer.
    pub(crate) fn recent_completed(
        &mut self,
        side: usize,
        visits: Vec<Visit>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .panes
            .get(side)
            .is_some_and(|host| host.as_ref().is_some_and(|host| !host.stopped))
        {
            self.recent_open(side, visits, window, cx);
        }
    }

    /// A navigation on any side leaves the history ask of the side it moved behind, so its late reply is dropped.
    pub(crate) fn cancel_recent_ask(&mut self, side: usize, cx: &mut Context<Self>) {
        if let Some(join) = &self.rail {
            join.places
                .update(cx, |producer, _| producer.cancel_recent(side));
        }
    }

    /// Refuses folder-only actions in Recent, whether opened from the rail or through a `recent:` place.
    pub(crate) fn recent_refuses(
        &mut self,
        side: usize,
        key: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.panes[side].as_ref().is_some_and(|host| {
            host.recent.on || flea::ui_logic_picker_portal::is_recent(host.pane.place())
        }) {
            return false;
        }
        let text = self.locale.catalog.tr(key);
        self.status_note(&text, cx);
        cx.notify();
        true
    }

    pub(crate) fn recent_open(
        &mut self,
        side: usize,
        visits: Vec<Visit>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_leave(side, window, cx);
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        if !host.recent.on {
            host.recent.stashed = Some((host.nav.order.key.into(), host.nav.order.desc));
            host.recent.from = host.pane.place().into();
        }
        host.recent.on = true;
        host.recent.paths = visits.iter().map(|visit| visit.path.clone()).collect();
        host.recent.visits = recent::visits(&visits);
        host.list
            .borrow_mut()
            .set_recent_place(Some(host.recent.visits.clone()));
        host.nav.order = Order {
            key: "mtime",
            desc: true,
        };
        host.nav.restore = None;
        host.nav.anchor = None;
        host.pane.swap.end(host.pane.now(), "recent");
        let _ = host.nav.list_paths(
            &mut host.pane,
            &host.recent.paths,
            host.search.owner.session.leaving,
        );
        self.run_navigation(side, window, cx);
        let keyboard_open = self
            .rail
            .as_ref()
            .is_some_and(|join| !join.places.read(cx).model.borrow().pointer_focus);
        if keyboard_open && side == self.focus_side {
            self.rail_focus(false, cx);
            window.focus(&self.focus, cx);
            self.keyboard.context = flea::keys::Context::Listing;
        }
        cx.notify();
    }

    /// Routes one navigation step while Recent stands; None lets the step run as a folder step.
    pub(crate) fn recent_step(
        &mut self,
        side: usize,
        step: &Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Result<bool, Refusal>> {
        let host = self.panes.get_mut(side)?.as_mut()?;
        if !host.recent.on {
            return None;
        }
        match step {
            Step::Back | Step::MouseBack | Step::Up => {
                let result = host.close_recent();
                self.run_navigation(side, window, cx);
                cx.notify();
                Some(result)
            }
            Step::Refresh(_) | Step::Fresh | Step::Reread | Step::Reload | Step::Retry => {
                if host.pane.busy() {
                    return Some(Err(Refusal::Busy));
                }
                match step {
                    Step::Refresh(select) => host.nav.pending_select = select.clone(),
                    Step::Fresh => host.nav.pending_select = None,
                    _ => {}
                }
                match &self.rail {
                    Some(join) => {
                        join.places.update(cx, |producer, cx| {
                            producer.side = side;
                            let file = producer.history_file();
                            producer.read_recent(file, cx);
                        });
                    }
                    None => {
                        host.pane.swap.end(host.pane.now(), "recent");
                        let _ = host
                            .nav
                            .list_paths(&mut host.pane, &host.recent.paths, false);
                        self.run_navigation(side, window, cx);
                    }
                }
                cx.notify();
                Some(Ok(true))
            }
            // A folder row or an Open leaves the history for that folder, which lists over it.
            Step::Open(_) | Step::OpenPlace(_) => {
                host.leave_recent();
                None
            }
            Step::OpenCursor
                if !host.pane.busy()
                    && host
                        .pane
                        .held
                        .row(host.pane.cursor)
                        .is_some_and(|row| row.dir) =>
            {
                host.leave_recent();
                None
            }
            // A file row opens as it does anywhere; the history stays standing behind it.
            Step::OpenCursor => None,
            // A history has no next folder and no order to forget.
            Step::Forward | Step::ForgetSort => Some(Ok(false)),
            // Sorting re-orders the history's own rows; leaving Recent still hands the folder's order back.
            Step::SortColumn(_) | Step::SortNext | Step::SortReverse => None,
        }
    }

    /// o on a Recent row: opens that row's folder with the row selected, as Recent's reveal does.
    pub(crate) fn recent_reveal(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side].as_mut().filter(|host| !host.pane.busy()) else {
            return;
        };
        let Some(row) = host.pane.held.row(host.pane.cursor) else {
            return;
        };
        let path = crate::navigation::history::join(host.pane.path(), &row.name);
        let Some(parent) = std::path::Path::new(&path)
            .parent()
            .and_then(|parent| parent.to_str())
        else {
            return;
        };
        host.leave_recent();
        host.nav.pending_select = Some(path.clone());
        let _ =
            host.nav
                .open_without_history(&mut host.pane, parent, Options::default(), None, false);
        self.run_navigation(side, window, cx);
        cx.notify();
    }
}
