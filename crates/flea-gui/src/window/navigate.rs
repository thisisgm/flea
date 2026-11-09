//! Navigation steps routed to one pane's navigator and executed on its backend, never on a frame.
use crate::{
    navigation::{watch::Gate, Command, Refusal},
    state::Shell,
};
use gpui::{Context, Window};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Open(String),
    OpenCursor,
    OpenPlace(String),
    Back,
    Forward,
    MouseBack,
    Up,
    Refresh(Option<String>),
    /// The re-list an operation's result owes: it supersedes a re-read in flight and lands on the first row with nothing marked.
    Fresh,
    /// The re-read that keeps the cursor's row and its marks by name, with no reload notice.
    Reread,
    Reload,
    Retry,
    SortColumn(String),
    SortNext,
    SortReverse,
    ForgetSort,
}

impl Shell {
    pub fn step(
        &mut self,
        side: usize,
        step: Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, Refusal> {
        if self.lifecycle.phase() != super::lifecycle::Phase::Open {
            return Ok(false);
        }
        if let Some(result) = self.covered_step(side, &step, window, cx) {
            return result;
        }
        if matches!(step, Step::Back | Step::MouseBack)
            && self.panes[side]
                .as_ref()
                .is_some_and(|host| host.search.owner.active())
        {
            self.search_update(side, None, |search, host| search.back(host), window, cx);
            return Ok(true);
        }
        // A refused or file row Enter keeps the search it was pressed in; only a folder it opens leaves it.
        let leaves = step != Step::OpenCursor
            || self.panes[side].as_ref().is_some_and(|host| {
                !host.pane.busy()
                    && host
                        .pane
                        .held
                        .row(host.pane.cursor)
                        .is_some_and(|row| row.dir)
            });
        if leaves {
            self.search_leave(side, window, cx);
        }
        let failure = self.locale.catalog.tr("navigation.save_failed");
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return Ok(false);
        };
        if host.stopped {
            return Ok(false);
        }
        host.prepare_grid_anchor();
        let (nav, pane, selected) = (&mut host.nav, &mut host.pane, &mut host.selected);
        let result = match step {
            Step::Open(path) => nav.open(pane, &path),
            Step::OpenCursor => nav.open_cursor(pane).map(|file| file.is_none()),
            Step::OpenPlace(path) => nav.open_place(pane, &path),
            Step::Back => nav.back(pane),
            Step::Forward => nav.forward(pane),
            Step::MouseBack => nav.mouse_back(pane),
            Step::Up => nav.parent(pane),
            Step::Refresh(select) => nav.refresh(pane, select),
            Step::Fresh => nav.fresh(pane),
            Step::Reread => Ok(nav.reread(pane, selected, false, false)),
            Step::Reload => nav.reload(pane, selected),
            Step::Retry => nav.retry(pane, selected),
            Step::SortColumn(key) => nav.sort_column(pane, &key, selected, failure),
            Step::SortNext => nav.sort_next(pane, selected, failure),
            Step::SortReverse => nav.sort_reverse(pane, selected, failure),
            Step::ForgetSort => nav.forget_sort(pane, failure),
        };
        if let Err(refusal) = &result {
            host.status = self.locale.catalog.tr(refusal.key()).into();
            // A busy refusal also reaches the footer, where the listing's presses and keys say it.
            if *refusal == Refusal::Busy {
                let text = host.status.clone();
                self.status_note(&text, cx);
            }
        }
        self.run_navigation(side, window, cx);
        cx.notify();
        result
    }

    // Producers that own the rows report here; a released gate pays the owed re-read and held sort.
    pub fn set_gate(
        &mut self,
        side: usize,
        gate: Gate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let failure = self.locale.catalog.tr("navigation.save_failed");
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        host.nav.gate = Gate {
            paths_pending: host.nav.gate.paths_pending,
            search: gate.search || host.search.owner.active(),
            ..gate
        };
        let _ = host
            .nav
            .apply_pending_sort(&mut host.pane, &mut host.selected, failure);
        let now = host.pane.now();
        host.nav.settle(&host.pane, now);
        self.run_navigation(side, window, cx);
        self.arm_watch(side, window, cx);
        self.columns_landed(side, window, cx);
        self.settle_settings(window, cx);
    }

    pub(crate) fn after_navigation(
        &mut self,
        side: usize,
        before: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        self.surfaces[side].filesystem.enter(&host.path);
        let landed = host.nav.landed;
        host.nav
            .after_reply(&mut host.pane, before, &mut host.selected);
        let sorted = host.nav.committed_sort.take();
        // Nav.js applyPendingSelect only(index, true): the landed row is a lone mark, so v keeps it and a move carries it.
        if let Some((listing, row)) = host.nav.landed.filter(|now| landed != Some(*now)) {
            if listing == host.pane.held.listing && host.selected == [row] {
                host.input.selection.only(&mut host.selected, row);
            }
        }
        host.take_lone_mark();
        super::outline::keep_lone(host);
        if let Some(changed) = host.nav.reloaded.take() {
            let notice = crate::strips::status_host::reloaded(&self.locale.catalog, changed);
            self.status_messages(cx, |messages, now| {
                messages.say(
                    &notice,
                    false,
                    "",
                    flea::ui_logic_status_feedback::status::Hint::None,
                    now,
                )
            });
        }
        if let Some(order) = sorted {
            self.remember_global_sort(side, order);
        }
        self.tab_landed(side, window, cx);
        self.run_navigation(side, window, cx);
        self.replay_queued(side, window, cx);
        self.arm_watch(side, window, cx);
        if self.panes[side]
            .as_ref()
            .is_some_and(|host| host.path != before && !host.pane.busy())
        {
            self.remember_dual();
        }
        if side == self.focus_side {
            self.settings_folder_arrived(window, cx);
        }
        self.columns_landed(side, window, cx);
        self.settle_settings(window, cx);
    }

    // PaneSwap.qml: a completed user sort becomes the window's fallback while folder memory is off.
    fn remember_global_sort(&mut self, side: usize, order: crate::navigation::folder_state::Order) {
        if self.view == super::startup::View::Dual {
            return;
        }
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let mut folders = host.nav.folders.borrow_mut();
        if host.pane.state.search || folders.remember_sort || folders.global == order {
            return;
        }
        let patch = format!(
            "{{\"sort\":{{\"key\":\"{}\",\"reverse\":{}}}}}",
            crate::navigation::folder_state::stored_key(order.key),
            order.desc
        );
        let Ok(data) = flea::uistate::patched(
            &self.launch.settings.data,
            &flea::jsondoc::parse(&patch).unwrap(),
        ) else {
            return;
        };
        folders.global = order;
        drop(folders);
        self.launch.settings.data = data;
        host.nav
            .persist(patch, self.locale.catalog.tr("navigation.save_failed"));
    }

    pub(crate) fn run_navigation(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        host.input.selection.sync(&host.selected);
        let commands = host.nav.take();
        let mut listed = false;
        let fresh = commands
            .iter()
            .any(|command| matches!(command, Command::List(_)));
        for command in &commands {
            crate::navigation::sort::apply_own_patch(&mut self.launch.settings.data, command);
            if let Command::List(listing) | Command::ListPaths(listing, _) = command {
                super::columns::requested(host, listing);
                listed = true;
                host.waiting = false;
                host.changed(&self.locale);
            }
            let Some(sent) = super::outline::send_navigation(host, command) else {
                continue;
            };
            if let Err(error) = sent {
                if host.pane.fail("backend", "", "", 0, host.pane.now()) {
                    self.surfaces[side].refuse(crate::window::surfaces::Refusal {
                        operation: "backend".into(),
                        message: String::new(),
                        mode: 0,
                    });
                }
                host.input.queued = None;
                host.changed(&self.locale);
                host.record_error(error, &self.locale);
                break;
            }
        }
        if listed {
            let name = self.locale.catalog.tr("places.row.recent");
            self.path_labels[side] = super::recent::path_label(host, &name);
        }
        if fresh {
            self.cancel_recent_ask(side, cx);
        }
        if listed {
            self.arm_transition(side, window, cx);
            self.arm_listing_wait(side, window, cx);
        }
    }

    /// PaneStates' PointHandler: a held press owns the rows, so a watched re-read waits for its release.
    pub(crate) fn hold_pointer(
        &mut self,
        side: usize,
        held: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes.get(side).and_then(Option::as_ref) else {
            return;
        };
        if host.nav.gate.pointer_held != held {
            let gate = Gate {
                pointer_held: held,
                ..host.nav.gate
            };
            self.set_gate(side, gate, window, cx);
        }
    }

    // A lost release cancels gesture state before allowing a watched listing to replace its rows.
    pub(crate) fn cancel_pointer(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side]
            .as_mut()
            .filter(|host| host.nav.gate.pointer_held)
        else {
            return;
        };
        host.input.cancel(&mut host.selected, &mut host.pane.cursor);
        host.input.press = None;
        host.input.pulled = None;
        self.hold_pointer(side, false, window, cx);
        cx.notify();
    }

    pub(crate) fn arm_watch(&mut self, side: usize, window: &Window, cx: &mut Context<Self>) {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        let Some(due) = host.nav.watch.due else {
            host.watch_task = None;
            return;
        };
        if host
            .watch_task
            .as_ref()
            .is_some_and(|(armed, _)| *armed == due)
        {
            return;
        }
        let wait = due.saturating_sub(host.pane.now());
        let task = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(wait))
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |shell, cx| shell.watch_due(side, window, cx))
            });
        });
        host.watch_task = Some((due, task));
    }

    /// The window's two activation observers: blur cancels a divider drag, a return re-reads blind folders.
    pub(crate) fn subscribe_activation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.subscribe_blur(window, cx);
        self.subscribe_return(window, cx);
    }

    /// A window coming back to the front re-reads every shown folder inotify cannot hear, through the watch's own route (#324).
    fn subscribe_return(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut front = crate::navigation::watch::Front::default();
        front.turn(window.is_window_active());
        cx.observe_window_activation(window, move |shell, window, cx| {
            if front.turn(window.is_window_active()) {
                shell.window_returned(window, cx);
            }
        })
        .detach();
    }

    pub(crate) fn window_returned(&mut self, window: &Window, cx: &mut Context<Self>) {
        for side in 0..self.panes.len() {
            let Some(host) = self.panes[side].as_mut() else {
                continue;
            };
            let fs = self.surfaces[side]
                .filesystem
                .facts
                .as_ref()
                .filter(|facts| facts.path == host.path)
                .map_or("", |facts| facts.name.as_str());
            if !crate::navigation::watch::blind(fs, host.sizes.class().unwrap_or_default()) {
                continue;
            }
            let now = host.pane.now();
            let path = host.pane.path().to_owned();
            host.nav.changed(&host.pane, &path, now);
            self.arm_watch(side, window, cx);
        }
    }

    fn watch_due(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        host.watch_task = None;
        host.prepare_grid_anchor();
        let now = host.pane.now();
        if host.nav.fire_watch(&mut host.pane, &host.selected, now) {
            self.run_navigation(side, window, cx);
            cx.notify();
        }
        self.arm_watch(side, window, cx);
    }
}

#[cfg(test)]
#[path = "navigate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "navigate_sort_tests.rs"]
mod sort_tests;
