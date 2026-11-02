//! Timer-driven pane transitions; no animation loop or filesystem work waits for a listing.
use crate::{
    pane::{swap::Options, Pane},
    state::Shell,
};
use gpui::{Context, Window};

impl Shell {
    pub fn pane(&self, side: usize) -> Option<&Pane> {
        self.panes.get(side)?.as_ref().map(|host| &host.pane)
    }

    /// The pane this window draws on this side: the second only in the dual view.
    pub(crate) fn shown_pane(&self, side: usize) -> Option<&super::pane::PaneHost> {
        if side == 1 && self.view != super::startup::View::Dual {
            return None;
        }
        self.panes.get(side)?.as_ref()
    }

    /// The loading mark is drawn while a listing is out past its hold, as LoadingState drew it.
    pub(crate) fn mark_drawn(&self, side: usize) -> bool {
        self.shown_pane(side).is_some_and(|host| {
            let pane = &host.pane;
            pane.busy()
                && pane.state.kind == crate::pane::states::Kind::Loading
                && pane.swap.fell_back
        })
    }

    pub fn pane_text(&self, side: usize) -> Option<&crate::pane::states::Text> {
        self.panes.get(side)?.as_ref().map(|host| &host.state_text)
    }

    /// Steps a pane's caption words only while that pane is drawn in a visible window.
    pub fn pane_empty_beat(
        &mut self,
        side: usize,
        reduced_motion: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let shown = self.shown_pane(side).is_some() && window.is_visible();
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return false;
        };
        if host.pane.busy() || !host.pane.state.empty_beat(reduced_motion, shown) {
            return false;
        }
        host.changed(&self.locale);
        cx.notify();
        true
    }

    pub fn navigate(
        &mut self,
        side: usize,
        path: String,
        select: Option<String>,
        options: Options,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.lifecycle.phase() != super::lifecycle::Phase::Open {
            return false;
        }
        self.search_leave(side, window, cx);
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return false;
        };
        if host.stopped || !host.pane.begin(path.clone(), options, host.pane.now()) {
            return false;
        }
        host.select = select.clone();
        host.waiting = false;
        host.changed(&self.locale);
        if let Some(backend) = &host.backend {
            if let Err(error) = backend.list(path, select, host.pane.first_count()) {
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
            }
        }
        self.arm_transition(side, window, cx);
        self.arm_listing_wait(side, window, cx);
        cx.notify();
        true
    }

    pub(crate) fn arm_listing_wait(
        &mut self,
        side: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = &self.panes[side] else {
            return;
        };
        let request = host.pane.list_requests;
        self.deadlines[side] = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(
                    super::startup::LISTING_WAIT_MS,
                ))
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |shell, cx| {
                    if let Some(host) = &mut shell.panes[side] {
                        if host.pane.list_requests == request
                            && host.pane.busy()
                            && !host.pane.swap.retains_rows()
                            && !host.stopped
                            && host.error.is_none()
                        {
                            host.waiting = true;
                            host.pane.state.kind = crate::pane::states::Kind::Waiting;
                            host.pane.swap.mark_shown(false, host.pane.now());
                            host.state_text = host
                                .pane
                                .state
                                .text(&shell.locale.catalog, host.pane.held.total);
                            host.status = shell.locale.catalog.tr("window.waiting").into();
                            if side == 0 {
                                shell.ensure_ipc_reader(window, cx);
                            }
                            cx.notify();
                        }
                    }
                })
            });
        }));
    }

    pub(crate) fn arm_transition(&mut self, side: usize, window: &Window, cx: &mut Context<Self>) {
        if self.lifecycle.phase() != super::lifecycle::Phase::Open {
            if let Some(host) = &mut self.panes[side] {
                host.transition = None;
            }
            return;
        }
        let feedback = self.presentation.deadline();
        let deadline_side = self.panes.iter().position(Option::is_some);
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        let pane_now = host.pane.now();
        let pane = host.pane.swap.deadline(pane_now).map(|delay| {
            host.pane
                .clock
                .at(pane_now.saturating_add(delay.as_millis() as u64))
        });
        let at = match (pane, feedback.filter(|_| deadline_side == Some(side))) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        if host.transition_at == at && (at.is_none() || host.transition.is_some()) {
            return;
        }
        host.transition_at = at;
        host.transition = at.map(|at| {
            cx.spawn_in(window, async move |this, cx| {
                let delay = at.saturating_duration_since(cx.background_executor().now());
                cx.background_executor().timer(delay).await;
                let _ = cx.update(|window, cx| {
                    this.update(cx, |shell, cx| {
                        if let Some(host) = &mut shell.panes[side] {
                            host.transition_at = None;
                            host.transition = None;
                        }
                        shell.sync_search_coverage();
                        shell.presentation.expire(cx.background_executor().now());
                        shell.sync_search_coverage();
                        // One window deadline may retire associations for either pane.
                        for side in 0..2 {
                            shell.advance_transition(side, window, cx);
                            shell.arm_transition(side, window, cx);
                        }
                    })
                });
            })
        });
    }

    pub(crate) fn advance_transition(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tick_transition(side, true, window, cx);
    }

    pub(crate) fn retick_feedback(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pane(side).is_some_and(|pane| {
            pane.swap
                .pending
                .as_ref()
                .is_some_and(|pending| pending.ready.is_some())
        }) {
            self.tick_transition(side, false, window, cx);
        }
        self.arm_transition(side, window, cx);
    }

    fn tick_transition(
        &mut self,
        side: usize,
        timer: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lifecycle.phase() != super::lifecycle::Phase::Open {
            return;
        }
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        let before = host.pane.path().to_owned();
        let changed = host.pane.tick(host.pane.now());
        if changed {
            host.changed(&self.locale);
            self.path_labels[side] = host.path.clone().into();
            self.seam.rows(&host.pane);
            if host.answered {
                self.deadlines[side] = None;
            }
            cx.notify();
        }
        let path_changed = host.pane.path() != before;
        self.ipc_listing_answered(side, window, cx);
        if timer || changed || path_changed {
            self.search_landed(side, false, window, cx);
            self.settle_dual(window, cx);
            self.after_navigation(side, &before, window, cx);
        }
    }

    pub fn pane_mark_shown(
        &mut self,
        side: usize,
        shown: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) {
            let loading = host.pane.state.kind == crate::pane::states::Kind::Loading;
            host.pane.swap.mark_shown(shown && loading, host.pane.now());
            self.arm_transition(side, window, cx);
        }
    }

    /// Pane zero answered or stopped, so the window's IPC readers open (v0.3.12 answers calls once its first listing lands).
    pub(crate) fn ipc_listing_answered(
        &mut self,
        side: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if side == 0
            && self.panes[0]
                .as_ref()
                .is_some_and(|host| host.answered || host.stopped)
        {
            self.ensure_ipc_reader(window, cx);
        }
    }
}
