//! The Trash opening uses the existing cap and minimum-show contract before it replaces the covered listing.
use crate::pane::{
    states::{Kind, State},
    swap::{Options, Swap},
};
use gpui::{Subscription, Task};
use std::{collections::VecDeque, time::Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    #[default]
    Closed,
    Holding,
    Loading,
    Ready,
}

pub struct Presentation {
    pub swap: Swap,
    pub stage: Stage,
    pub held_trash: bool,
    pub generation: u64,
    pub epoch: Instant,
    pub timer: Option<Task<()>>,
    pub feedback: Option<Subscription>,
    pub frames: VecDeque<(u64, Stage, u64, bool, u64)>,
    pub last_presented: u64,
    pub holds: u64,
    pub fallbacks: u64,
    pub loading_frames: u64,
    held_generation: u64,
    fallback_generation: u64,
    pub blank_frames: u64,
    pub blink_backs: u64,
    previous: Option<(Stage, u64, u64)>,
}

impl Presentation {
    pub fn new(now: Instant) -> Self {
        Self {
            swap: Swap::default(),
            stage: Stage::Closed,
            held_trash: false,
            generation: 0,
            epoch: now,
            timer: None,
            feedback: None,
            frames: VecDeque::with_capacity(128),
            last_presented: 0,
            holds: 0,
            fallbacks: 0,
            held_generation: 0,
            fallback_generation: 0,
            loading_frames: 0,
            blank_frames: 0,
            blink_backs: 0,
            previous: None,
        }
    }
    pub fn now(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.epoch).as_millis() as u64
    }
    pub fn begin(&mut self, held: bool, now: u64) {
        self.held_trash = false;
        self.generation += 1;
        self.stage = Stage::Holding;
        self.swap.begin(
            "trash:///".into(),
            350,
            &State {
                kind: if held { Kind::Ready } else { Kind::Loading },
                ..Default::default()
            },
            Options {
                supersede: true,
                ..Default::default()
            },
            now,
        );
    }
    pub fn ready(&mut self, now: u64) {
        self.swap.end(now, "ready");
        self.stage = Stage::Ready;
    }
    pub fn close(&mut self, now: u64) {
        self.swap.end(now, "closed");
        self.stage = Stage::Closed;
        self.timer = None;
    }
    pub fn frame(&mut self, serial: u64, blank: bool, source_request: u64) {
        if self.frames.back().is_some_and(|frame| frame.0 == serial) {
            self.frames.pop_back();
        }
        if self.frames.len() == 128 {
            self.frames.pop_front();
        }
        self.frames
            .push_back((serial, self.stage, self.generation, blank, source_request));
    }
    pub fn presented(&mut self, serial: u64) {
        if serial <= self.last_presented {
            return;
        }
        self.last_presented = serial;
        let Some((_, stage, generation, blank, source_request)) = self
            .frames
            .iter()
            .rev()
            .find(|frame| frame.0 <= serial)
            .copied()
        else {
            return;
        };
        if stage == Stage::Holding && self.held_generation != generation {
            self.holds += 1;
            self.held_generation = generation;
        }
        if stage == Stage::Loading && self.fallback_generation != generation {
            self.fallbacks += 1;
            self.fallback_generation = generation;
        }
        if stage == Stage::Loading {
            self.loading_frames += 1;
        }
        if blank && stage == Stage::Ready {
            self.blank_frames += 1;
        }
        if let Some((previous, old_generation, old_source_request)) = self.previous {
            if generation == old_generation
                && ((previous == Stage::Ready && stage < previous && stage != Stage::Closed)
                    || (stage == Stage::Ready && source_request < old_source_request))
            {
                self.blink_backs += 1;
            }
        }
        self.previous = Some((stage, generation, source_request));
        while self.frames.front().is_some_and(|frame| frame.0 < serial) {
            self.frames.pop_front();
        }
    }
}
