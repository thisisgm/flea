//! The shared lender's raw source and decoder state, guarded by the current listing, facts and policy.
use super::{Map, Pictures, Slot};
use crate::list::pane::Held;

#[derive(Clone, Copy)]
#[repr(i32)]
enum ImageStatus {
    Null = 0,
    Ready = 1,
    Loading = 2,
    Error = 3,
}

impl Map {
    fn current(&self, held: &Held, row: usize) -> Option<&Slot> {
        let facts = held.row(row)?;
        if self.listing != held.listing || !self.policy.is_some_and(|policy| policy.allowed(facts))
        {
            return None;
        }
        self.slots
            .get(&row)
            .filter(|slot| slot.identity().is_some_and(|identity| identity.is(facts)))
    }
}

impl Pictures {
    pub(crate) fn file(&self, held: &Held, row: usize) -> String {
        let map = self.map.borrow();
        match map.current(held, row) {
            Some(
                Slot::Decoding { file, .. } | Slot::Ready { file, .. } | Slot::Failed { file, .. },
            ) => file.clone(),
            _ => String::new(),
        }
    }

    // Qt Image.Null, Image.Ready, Image.Loading and Image.Error are the frozen IPC values.
    pub(crate) fn status(&self, held: &Held, row: usize) -> i32 {
        (match self.map.borrow().current(held, row) {
            Some(Slot::Ready { .. }) => ImageStatus::Ready,
            Some(Slot::Decoding { .. }) => ImageStatus::Loading,
            Some(Slot::Failed { .. }) => ImageStatus::Error,
            _ => ImageStatus::Null,
        }) as i32
    }
}
