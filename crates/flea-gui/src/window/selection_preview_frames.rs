//! Preview counters associate each painted scene with its exact presented commit.
use super::*;
use gpui::{FrameFeedbackEvent, FrameOutcome, Window};
use std::collections::VecDeque;

#[derive(Clone, Copy)]
pub(crate) struct Stamp {
    pub generation: u64,
    pub loading: bool,
    pub held: bool,
    pub blank: bool,
}

/// The last native animation paint attempt, tied to its preview generation and draw.
pub(crate) struct AnimationPaint {
    pub generation: u64,
    pub index: usize,
    pub draw: Option<u64>,
    pub error: Option<String>,
}

#[derive(Default)]
pub(crate) struct Frames {
    pub animation: Option<AnimationPaint>,
    pub available: bool,
    pub presented: u64,
    pub loading: u64,
    pub held: u64,
    pub blank: u64,
    pub blink_backs: u64,
    pub lost: u64,
    last: (u64, u64),
    drawn: VecDeque<(u64, Stamp)>,
    pending: VecDeque<(u64, Stamp)>,
}
impl Frames {
    pub(crate) fn stage(&mut self, id: u64, stamp: Stamp) {
        if self.drawn.back().is_some_and(|(draw, _)| *draw == id) {
            return;
        }
        if self.drawn.len() == 512 {
            self.drawn.pop_front();
        }
        self.drawn.push_back((id, stamp));
    }
    fn receive(&mut self, event: FrameFeedbackEvent) {
        match event {
            FrameFeedbackEvent::Submitted { draw_id, frame_id } => {
                if let Some((_, stamp)) = self.drawn.iter().find(|(id, _)| *id == draw_id) {
                    if self.pending.len() == 512 {
                        self.pending.pop_front();
                        self.lost += 1;
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
                if !matches!(terminal.outcome, FrameOutcome::Presented(_)) {
                    return;
                }
                self.presented += 1;
                self.loading += u64::from(stamp.loading);
                self.held += u64::from(stamp.held);
                self.blank += u64::from(stamp.blank);
                if terminal.frame_id > self.last.0 && !stamp.held && !stamp.loading {
                    self.blink_backs += u64::from(stamp.generation < self.last.1);
                    self.last = (terminal.frame_id, stamp.generation.max(self.last.1));
                }
            }
        }
    }
}
impl SelectionPreview {
    pub(crate) fn observe_frames(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let frames = self.frames.clone();
        self.frame_feedback = window
            .observe_frame_feedback(cx, move |event, _, _| frames.borrow_mut().receive(event));
        self.frames.borrow_mut().available = self.frame_feedback.is_some();
    }
}
