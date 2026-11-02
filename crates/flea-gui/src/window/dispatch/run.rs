//! Runs one order through the families that claim it.
use super::{Order, FAMILIES};
use crate::state::Shell;
use gpui::{Context, Window};

impl Shell {
    /// Runs the order through its family; one nobody ran is emitted for observers, and an unclaimed menu choice is refused.
    pub(crate) fn dispatch(&mut self, order: Order, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch_claimed(order, window, cx);
    }

    /// `dispatch`, answering whether a family ran the order; an outside `flea --action` reads the refusal from it.
    pub(crate) fn dispatch_claimed(
        &mut self,
        order: Order,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // Pane.qml act: every dispatched action ends a pending slow click.
        self.slow_cancel();
        if FAMILIES
            .iter()
            .any(|family| family(self, &order, window, cx))
        {
            return true;
        }
        match order {
            Order::Action(event) => {
                let text = self.locale.catalog.tr("feedback.action.unavailable");
                self.status_note(&text, cx);
                cx.emit(event)
            }
            Order::Network(event) => cx.emit(event),
            Order::Menu(chosen) => cx.emit(chosen),
            Order::Place(request) => cx.emit(request),
            Order::Device(request) => cx.emit(request),
        }
        false
    }
}
