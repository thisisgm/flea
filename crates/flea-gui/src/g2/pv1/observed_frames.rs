//! Paint stamps are credited only by the presentation terminal for their own submitted frame.
use super::{observations::Shared, QuickLook};
use gpui::{Context, FrameFeedbackEvent, FrameOutcome, Window};
use std::collections::VecDeque;

#[derive(Clone, Copy)]
pub(crate) struct Stamp {
    pub visible: bool,
    pub ready: bool,
    pub holding: bool,
    pub loading: bool,
}

#[derive(Default)]
pub(crate) struct Frames {
    drawn: VecDeque<(u64, Stamp)>,
    pending: VecDeque<(u64, Stamp)>,
}

impl Frames {
    pub fn stage(&mut self, id: u64, stamp: Stamp) {
        if self.drawn.back().is_some_and(|(draw, _)| *draw == id) {
            return;
        }
        if self.drawn.len() == 512 {
            self.drawn.pop_front();
        }
        self.drawn.push_back((id, stamp));
    }

    pub fn receive(&mut self, event: FrameFeedbackEvent, observations: &Shared) {
        match event {
            FrameFeedbackEvent::Submitted { draw_id, frame_id } => {
                if let Some((_, stamp)) = self.drawn.iter().find(|(id, _)| *id == draw_id) {
                    if self.pending.len() == 512 {
                        self.pending.pop_front();
                    }
                    self.pending.push_back((frame_id, *stamp));
                }
            }
            FrameFeedbackEvent::Terminal(terminal) => {
                let Some(at) = self
                    .pending
                    .iter()
                    .position(|(id, _)| *id == terminal.frame_id)
                else {
                    return;
                };
                let (_, stamp) = self.pending.remove(at).unwrap();
                if matches!(terminal.outcome, FrameOutcome::Presented(_)) {
                    observations.borrow_mut().frame(
                        stamp.visible,
                        stamp.ready,
                        stamp.holding,
                        stamp.loading,
                    );
                }
            }
        }
    }
}

impl QuickLook {
    pub(super) fn observe_frames(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.observation_feedback.is_some() {
            return;
        }
        let frames = self.observation_frames.clone();
        let observations = self.observations.clone();
        self.observation_feedback = window.observe_frame_feedback(cx, move |event, _, _| {
            frames.borrow_mut().receive(event, &observations);
        });
    }
}
