//! Filter040, the name filter over the whole listing: the controller behind `filter::PaneSeam`, the `wire` answer, the key line, the strip and the washed runs.
pub mod filter;
#[cfg(test)]
mod i50_query_tests;
pub(crate) mod input;
pub mod keys;
pub mod present;
pub mod strip;
#[cfg(test)]
mod tests_flow;
#[cfg(test)]
mod tests_scope;
#[cfg(test)]
mod tests_strip;
#[cfg(test)]
mod tests_wire;
pub mod wire;

use gpui::{Context, IntoElement, Render, Window};
use std::{cell::RefCell, rc::Rc};

/// The strip as a view for `Slots::strips`; the owner rewrites `state.text` and notifies when the filter moves.
pub struct StripView {
    pub state: Rc<RefCell<strip::StripState>>,
}

impl Render for StripView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        strip::StripElement::new(Rc::clone(&self.state))
    }
}
