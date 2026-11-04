//! Joins immutable painted scenes to successful submissions and exact terminal feedback.
use super::lifecycle::Phase;
use crate::{
    pane::{
        presentation::{Content, Observed},
        states::Kind,
    },
    state::Shell,
};
use gpui::{Context, FrameFeedbackEvent, FrameOutcome, Subscription, Window, WindowId};
use std::{cell::RefCell, collections::VecDeque, rc::Rc, time::Instant};

mod activity;
#[cfg(test)]
pub(crate) mod perf_tests;
mod policy;
mod readback;
mod snapshot;
pub(crate) mod timing;

const RETAINED: usize = 512;

struct PaneFrame {
    clock: crate::pane::clock::Clock,
    content: Rc<Content>,
    observed: Rc<RefCell<Observed>>,
    loading: bool,
    blank: bool,
    list: Rc<RefCell<crate::list::ListState>>,
    ready: bool,
}

#[derive(Clone)]
struct Draw {
    id: u64,
    epoch: u64,
    panes: [Option<Rc<PaneFrame>>; 2],
}

struct Submitted {
    id: u64,
    draw: Draw,
    at: Instant,
    monotonic: std::time::Duration,
    visible: bool,
    waiting: bool,
}

impl Submitted {
    fn release_wait(&mut self) {
        if !self.waiting {
            return;
        }
        for frame in self
            .draw
            .panes
            .iter()
            .flatten()
            .filter(|frame| frame.loading)
        {
            let mut observed = frame.observed.borrow_mut();
            observed.outstanding_loading -= 1;
        }
        self.waiting = false;
    }

    fn unavailable(mut self) {
        let waited = self.waiting;
        self.release_wait();
        for frame in self.draw.panes.iter().flatten() {
            let mut observed = frame.observed.borrow_mut();
            observed.unavailable += 1;
            // Continued animation submissions cannot extend one generation's unresolved wait forever.
            observed.wait_unavailable |= waited && frame.loading;
        }
    }
}

#[derive(Default)]
pub(crate) struct Presentation {
    pub observer: Option<Subscription>,
    legacy_loading: [bool; 2],
    visibility: Option<Subscription>,
    window: Option<WindowId>,
    epoch: u64,
    rendered: [Option<Rc<PaneFrame>>; 2],
    snapshots: RefCell<[Option<snapshot::Cached>; 2]>,
    staged: RefCell<Option<Draw>>,
    retained: Option<Draw>,
    pending: VecDeque<Submitted>,
    last_submitted: u64,
    lost_metadata: usize,
    unmatched_terminals: usize,
    frames: u64,
    discarded: u64,
    #[cfg(test)]
    snapshot_builds: std::cell::Cell<u64>,
    pub(crate) timing: timing::Timing,
    pub(crate) activity: activity::Activity,
}

impl Presentation {
    pub fn invalidate(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        for frame in &mut self.pending {
            frame.release_wait();
        }
        // Keep retained scene facts: a later submission must describe that cached scene.
    }

    pub fn close(&mut self) {
        self.invalidate();
        self.lost_metadata += self.pending.len();
        for frame in self.pending.drain(..) {
            frame.unavailable();
        }
        self.rendered = [None, None];
        *self.snapshots.get_mut() = [None, None];
        *self.staged.get_mut() = None;
        self.retained = None;
        self.observer = None;
        self.visibility = None;
        self.activity.notified = None;
        self.window = None;
        self.legacy_loading = [false; 2];
    }

    pub fn painted(&self, window: &Window, eligible: [bool; 2]) {
        if !window.is_visible() || !eligible.into_iter().any(|side| side) {
            self.staged.borrow_mut().take();
            return;
        }
        let Some(id) = window.current_draw_id() else {
            return;
        };
        if self.observer.is_none() {
            return;
        }
        *self.staged.borrow_mut() = Some(Draw {
            id,
            epoch: self.epoch,
            panes: [0, 1].map(|side| {
                self.rendered[side]
                    .as_ref()
                    .filter(|_| eligible[side])
                    .map(|frame| self.snapshot(side, frame))
            }),
        });
    }

    fn submitted(&mut self, draw_id: u64, id: u64, visible: bool, now: Instant) {
        if id == 0 || id <= self.last_submitted {
            return;
        }
        self.last_submitted = id;
        let draw = if self
            .staged
            .borrow()
            .as_ref()
            .is_some_and(|draw| draw.id == draw_id)
        {
            self.staged.get_mut().take()
        } else {
            self.retained
                .as_ref()
                .filter(|draw| draw.id == draw_id)
                .cloned()
        };
        let Some(draw) = draw else {
            self.lost_metadata += 1;
            return;
        };
        self.retained = Some(draw.clone());
        if self.pending.len() == RETAINED {
            self.pending.pop_front().unwrap().unavailable();
            self.lost_metadata += 1;
        }
        let waiting = visible && draw.epoch == self.epoch;
        for frame in draw.panes.iter().flatten().filter(|frame| frame.loading) {
            let mut observed = frame.observed.borrow_mut();
            observed.loading_submissions += u64::from(visible);
            observed.outstanding_loading += usize::from(waiting);
        }
        self.pending.push_back(Submitted {
            id,
            draw,
            at: now,
            monotonic: self.timing.now(),
            visible,
            waiting,
        });
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.pending
            .front()
            .map(|frame| frame.at + crate::visual::FRAME_WAIT)
    }

    pub(crate) fn expire(&mut self, now: Instant) {
        while self.pending.front().is_some_and(|frame| {
            now.saturating_duration_since(frame.at) >= crate::visual::FRAME_WAIT
        }) {
            self.pending.pop_front().unwrap().unavailable();
            self.lost_metadata += 1;
        }
    }
}

impl Shell {
    pub(crate) fn follow_frame_feedback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.presentation.window.is_some() || self.lifecycle.phase() != Phase::Open {
            return;
        }
        self.presentation.window = Some(window.window_handle().window_id());
        crate::bench_thumbs::observe(window, cx);
        self.presentation.pending = VecDeque::with_capacity(RETAINED);
        self.presentation.activity.follow(cx);
        let weak = cx.entity().downgrade();
        self.presentation.observer = window.observe_frame_feedback(cx, move |event, window, cx| {
            let _ = weak.update(cx, |shell, cx| {
                shell.presentation_feedback(event, window, cx)
            });
        });
        let weak = cx.entity().downgrade();
        self.presentation.visibility =
            Some(window.observe_window_visibility(move |_, window, cx| {
                let _ = weak.update(cx, |shell, cx| {
                    shell.presentation.invalidate();
                    for side in 0..2 {
                        if !window.is_visible() {
                            if let Some(host) = &mut shell.panes[side] {
                                host.pane.swap.mark_shown(false, host.pane.now());
                            }
                        }
                        shell.retick_feedback(side, window, cx);
                    }
                    cx.notify();
                });
            }));
    }

    pub(crate) fn remember_paint(&mut self, window: &Window, cx: &gpui::App) {
        *self.presentation.snapshots.get_mut() = [None, None];
        for side in 0..2 {
            let eligible = self.feedback_eligible(side, window, cx);
            if let Some(host) = &self.panes[side] {
                host.pane.swap.observed.borrow_mut().available =
                    eligible && self.presentation.observer.is_some();
            }
            self.presentation.rendered[side] =
                self.shown_pane(side).filter(|_| eligible).map(|host| {
                    host.pane.swap.observed.borrow_mut().available =
                        self.presentation.observer.is_some();
                    Rc::new(PaneFrame {
                        clock: host.pane.clock.clone(),
                        content: Rc::new(Content {
                            identity: host.pane.identity(),
                            generation: host.pane.list_requests,
                            search_generation: host.pane.swap.session.generation,
                            search_scene: host.search_scene(),
                            held: host.pane.held.clone(),
                            path: self.path_labels[side].clone(),
                            cursor: host.pane.cursor,
                            drawn_cursor: None,
                            painted_rows: Vec::new(),
                            viewport: [
                                f32::from(window.viewport_size().width),
                                f32::from(window.viewport_size().height),
                                window.scale_factor(),
                            ],
                            font: host.list.borrow().typeface.font.family.clone(),
                            text_size: f32::from(host.list.borrow().typeface.body),
                            theme: cx
                                .try_global::<crate::palette::Theme>()
                                .copied()
                                .unwrap_or_default(),
                            selected: host.selected.clone().into(),
                            status: (side == self.focus_side).then(|| host.status.clone()),
                        }),
                        observed: host.pane.swap.observed.clone(),
                        list: host.list.clone(),
                        ready: host.pane.state.kind == Kind::Ready,
                        loading: self.mark_drawn(side)
                            && !matches!(self.seam.mark(), crate::feedback::spinner::Arm::Hidden),
                        blank: host.pane.busy()
                            && host.pane.state.kind == Kind::Loading
                            && !host.pane.swap.fell_back,
                    })
                });
        }
    }

    fn presentation_feedback(
        &mut self,
        event: FrameFeedbackEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lifecycle.phase() != Phase::Open
            || self.presentation.window != Some(window.window_handle().window_id())
        {
            return;
        }
        let now = cx.background_executor().now();
        self.sync_search_coverage();
        self.presentation.expire(now);
        match event {
            FrameFeedbackEvent::Submitted { draw_id, frame_id } => {
                self.presentation
                    .submitted(draw_id, frame_id, window.is_visible(), now);
            }
            FrameFeedbackEvent::Terminal(terminal) => {
                let index = self
                    .presentation
                    .pending
                    .iter()
                    .position(|frame| frame.id == terminal.frame_id);
                if let Some(index) = index {
                    let mut submitted = self.presentation.pending.remove(index).unwrap();
                    submitted.release_wait();
                    self.observe_terminal(submitted, terminal.outcome, window, cx);
                } else {
                    self.presentation.unmatched_terminals += 1;
                }
            }
        }
        self.sync_search_coverage();
        for side in 0..2 {
            self.retick_feedback(side, window, cx);
        }
    }

    fn observe_terminal(
        &mut self,
        submitted: Submitted,
        outcome: FrameOutcome,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let FrameOutcome::Presented(time) = outcome else {
            self.presentation.discarded += 1;
            for frame in submitted.draw.panes.iter().flatten() {
                frame.observed.borrow_mut().discarded += 1;
            }
            return;
        };
        self.presentation.frames += 1;
        self.presentation
            .timing
            .presented(submitted.id, submitted.monotonic, time);
        for (side, frame) in submitted.draw.panes.into_iter().enumerate() {
            let Some(frame) = frame else { continue };
            if submitted.visible {
                self.search_presented(side, &frame.content, submitted.id, time);
            }
            {
                let mut observed = frame.observed.borrow_mut();
                observed.frames += 1;
                observed.loading += u64::from(submitted.visible && frame.loading);
                observed.blank += u64::from(submitted.visible && frame.blank);
                // IDs order submissions, never chronological presentation or observer arrival.
                if submitted.id > observed.last_id {
                    observed.last_id = submitted.id;
                    observed.observer_ms = Some(frame.clock.elapsed().as_millis() as u64);
                    observed.native_at = time.timestamp;
                    observed.clock_id = time.clock_id;
                    observed.last = Some(frame.content.clone());
                }
            }
            if submitted.visible
                && frame.loading
                && submitted.draw.epoch == self.presentation.epoch
                && window.is_visible()
                && self.feedback_eligible(side, window, cx)
                && self
                    .shown_pane(side)
                    .is_some_and(|host| policy::current(host, &frame))
                && self.mark_drawn(side)
            {
                self.pane_mark_shown(side, true, window, cx);
            }
        }
    }
}

#[cfg(test)]
#[path = "frame_feedback_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "frame_feedback_join_tests.rs"]
mod join_tests;
