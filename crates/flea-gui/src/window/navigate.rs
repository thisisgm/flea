//! Navigation steps routed to one pane's navigator and executed on its backend, never on a frame.
use crate::{
    navigation::{watch::Gate, Command, Refusal},
    state::Shell,
};
use gpui::{Context, Window};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Open(String),
    OpenPlace(String),
    Back,
    Forward,
    MouseBack,
    Up,
    Refresh(Option<String>),
    Reload,
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
            Step::OpenPlace(path) => nav.open_place(pane, &path),
            Step::Back => nav.back(pane),
            Step::Forward => nav.forward(pane),
            Step::MouseBack => nav.mouse_back(pane),
            Step::Up => nav.parent(pane),
            Step::Refresh(select) => nav.refresh(pane, select),
            Step::Reload => Ok(nav.reread(pane, selected, true, false)),
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
            ..gate
        };
        let _ = host
            .nav
            .apply_pending_sort(&mut host.pane, &mut host.selected, failure);
        let now = host.pane.now();
        host.nav.settle(&host.pane, now);
        self.run_navigation(side, window, cx);
        self.arm_watch(side, window, cx);
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
        host.nav
            .after_reply(&mut host.pane, before, &mut host.selected);
        self.run_navigation(side, window, cx);
        self.arm_watch(side, window, cx);
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
        let commands = host.nav.take();
        let mut listed = false;
        for command in &commands {
            if let Command::List(_) = command {
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
