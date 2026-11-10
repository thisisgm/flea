//! While Settings is open its card covers the window, and a chrome or rail mark under it is left undrawn rather than cut.
use gpui::{Bounds, Pixels};
use std::{cell::Cell, rc::Rc};

/// Settings updates its box on layout; the window clears it when closed, and marks read it as they paint.
#[derive(Clone, Default)]
pub struct Cover(Rc<Cell<Option<Bounds<Pixels>>>>);

impl Cover {
    pub fn set(&self, card: Option<Bounds<Pixels>>) {
        self.0.set(card);
    }

    /// A mark whose box meets the card is not drawn, so every mark shows whole or not at all.
    pub fn hides(&self, mark: Bounds<Pixels>) -> bool {
        self.0.get().is_some_and(|card| card.intersects(&mark))
    }
}
