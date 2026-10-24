//! The one place the window runs what a producer hands it: key and pointer actions, menu choices and rail requests.
mod navigation;

use super::input::ActionEvent;
use crate::{g2::mn1::join::Chosen, state::Shell};
use gpui::{Context, Window};

/// What a producer hands the window to run.
pub(crate) enum Order {
    Action(ActionEvent),
    Menu(Chosen),
    Place(crate::g2::pl4::producer::Request),
    Device(crate::g2::pl1::source::Request),
}

/// A family runs the orders it owns and answers whether it ran this one.
type Family = fn(&mut Shell, &Order, &mut Window, &mut Context<Shell>) -> bool;

/// Each family keeps its handlers in its own file beside this one and adds one line here.
const FAMILIES: &[Family] = &[navigation::run];

impl Shell {
    /// Runs the order through its family; one nobody ran is emitted for observers, and an unclaimed menu choice is refused.
    pub(crate) fn dispatch(&mut self, order: Order, window: &mut Window, cx: &mut Context<Self>) {
        if FAMILIES
            .iter()
            .any(|family| family(self, &order, window, cx))
        {
            return;
        }
        match order {
            Order::Action(event) => cx.emit(event),
            Order::Menu(chosen) => cx.emit(chosen),
            Order::Place(request) => cx.emit(request),
            Order::Device(request) => cx.emit(request),
        }
    }
}

#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod navigation_tests;
