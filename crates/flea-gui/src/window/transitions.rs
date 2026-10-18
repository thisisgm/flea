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

    pub fn pane_text(&self, side: usize) -> Option<&crate::pane::states::Text> {
        self.panes.get(side)?.as_ref().map(|host| &host.state_text)
    }

    pub fn pane_empty_beat(
        &mut self,
        side: usize,
        reduced_motion: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return false;
        };
        if host.pane.busy() || !host.pane.state.empty_beat(reduced_motion) {
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
                host.pane.fail("backend", "", "", 0, host.pane.now());
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
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        let delay = host.pane.swap.deadline(host.pane.now());
        host.transition = delay.map(|delay| {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let _ = cx.update(|window, cx| {
                    this.update(cx, |shell, cx| {
                        if let Some(host) = &mut shell.panes[side] {
                            if host.pane.tick(host.pane.now()) {
                                host.changed(&shell.locale);
                                shell.path_labels[side] = host.path.clone().into();
                                shell.seam.rows(&host.pane);
                                if host.answered {
                                    shell.deadlines[side] = None;
                                }
                                cx.notify();
                            }
                        }
                        shell.arm_transition(side, window, cx);
                    })
                });
            })
        });
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
}
