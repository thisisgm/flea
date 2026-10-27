//! Consecutive sequence identity; destructive confirmation policy remains with the action owner.
use super::{Context, SEQUENCES};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Seat {
    pub listing: u64,
    pub cursor: usize,
    pub selection: u64,
    pub view: u64,
    pub focus: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Sequence {
    pending: Option<(&'static str, Seat, Context, &'static str)>,
}

impl Sequence {
    pub fn clear(&mut self) {
        self.pending = None;
    }

    /// The armed action and the seat it was armed on, while a pair waits for its second press.
    pub fn pending(&self) -> Option<(&'static str, Seat)> {
        self.pending.map(|(action, seat, _, _)| (action, seat))
    }

    pub fn live(&self, seat: Seat, context: Context, preset: &'static str) -> bool {
        self.pending
            .is_some_and(|(_, armed, at, under)| armed == seat && at == context && under == preset)
    }

    pub fn advance(
        &mut self,
        action: &'static str,
        seat: Seat,
        context: Context,
        preset: &'static str,
    ) -> Option<&'static str> {
        let binding = SEQUENCES.iter().find(|row| row.arm == action && row.paired);
        let previous = self.pending.take();
        let Some(binding) = binding else {
            return Some(action);
        };
        let stamp = (action, seat, context, preset);
        if previous == Some(stamp) {
            return Some(binding.action);
        }
        self.pending = Some(stamp);
        None
    }
}
