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
    Reload,
    Retry,
    ToggleHidden,
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
                !host.pane.busy() && host.pane.held.row(host.pane.cursor).is_some_and(|row| row.dir)
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
            Step::Reload => nav.reload(pane, selected),
            Step::Retry => nav.retry(pane, selected),
            Step::ToggleHidden => nav.toggle_hidden(pane, failure),
            Step::SortColumn(key) => nav.sort_column(pane, &key, selected, failure),
            Step::SortNext => nav.sort_next(pane, selected, failure),
            Step::SortReverse => nav.sort_reverse(pane, selected, failure),
            Step::ForgetSort => nav.forget_sort(pane, failure),
        };
        if let Err(refusal) = &result {
            host.status = self.locale.catalog.tr(refusal.key()).into();
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
        // Nav.js applyPendingSelect only(index, true): the landed row is a lone mark, so v keeps it and a move carries it.
        if let Some((listing, row)) = host.nav.landed.filter(|now| landed != Some(*now)) {
            if listing == host.pane.held.listing && host.selected == [row] {
                host.input.selection.only(&mut host.selected, row);
            }
        }
        if let Some(changed) = host.nav.reloaded.take() {
            let notice = crate::strips::status_host::reloaded(&self.locale.catalog, changed);
            self.status_messages(cx, |messages, now| {
                messages.say(&notice, false, "", flea::ui_logic_fb1::status::Hint::None, now)
            });
        }
        self.tab_landed(side, window, cx);
        self.run_navigation(side, window, cx);
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
        for command in &commands {
            if let Command::List(listing) = command {
                super::columns::requested(host, listing);
                listed = true;
                host.waiting = false;
                host.changed(&self.locale);
            }
            let Some(backend) = &host.backend else {
                continue;
            };
            if let Err(error) = backend.command(command) {
                if host.pane.fail("backend", "", "", 0, host.pane.now()) {
                    self.surfaces[side].refuse(crate::window::surfaces::Refusal {
                        operation: "backend".into(),
                        message: String::new(),
                        mode: 0,
                    });
                }
                host.changed(&self.locale);
                host.record_error(error, &self.locale);
                break;
            }
        }
        if listed {
            self.path_labels[side] = host.path.clone().into();
            self.arm_transition(side, window, cx);
            self.arm_listing_wait(side, window, cx);
        }
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

    fn watch_due(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        host.watch_task = None;
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
