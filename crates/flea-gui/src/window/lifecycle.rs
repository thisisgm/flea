//! Per-window pane ownership and the asynchronous close barrier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Open,
    Draining,
    Closed,
}

#[derive(Clone, Debug, Default)]
pub struct Lifecycle {
    phase: Phase,
    live: [bool; 2],
    drained: [bool; 2],
}

impl Lifecycle {
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn live_count(&self) -> usize {
        self.live.into_iter().filter(|live| *live).count()
    }

    pub fn attach(&mut self, side: usize) -> bool {
        if self.phase != Phase::Open || side > 1 || self.live[side] {
            return false;
        }
        self.live[side] = true;
        self.drained[side] = false;
        true
    }

    pub fn begin_close(&mut self) -> bool {
        if self.phase != Phase::Open {
            return false;
        }
        self.phase = Phase::Draining;
        self.finish();
        true
    }

    pub fn drained(&mut self, side: usize) {
        if side < 2 {
            self.drained[side] = true;
        }
        self.finish();
    }

    fn finish(&mut self) {
        if self.phase == Phase::Draining
            && (0..2).all(|side| !self.live[side] || self.drained[side])
        {
            self.phase = Phase::Closed;
        }
    }
}
