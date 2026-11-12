//! A resolved key action from the dispatch point on, the path a key press and an outside action id share.
use super::ActionEvent;
use crate::{pane::requests, state::Shell, window::dispatch::Order};
use gpui::{Context, Window};

/// How a resolved key action ended: Kept leaves the key to propagate, Busy met a listing in flight, Unclaimed found no owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyRun {
    Ran,
    Kept,
    Busy,
    Unclaimed,
}

impl Shell {
    /// A resolved key action from the point a key and an outside `flea --action` share; Unclaimed is the status line's refusal.
    pub(crate) fn run_key_action(
        &mut self,
        action: &'static str,
        seat: flea::keys::sequence::Seat,
        pair_armed: bool,
        arm_live: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> KeyRun {
        let Some(host) = &self.panes[self.focus_side] else {
            return KeyRun::Unclaimed;
        };
        let listing = self.keyboard.context == flea::keys::Context::Listing;
        // Horizontal extension belongs to Grid; other listing views leave these keys idle.
        if listing
            && self.view != crate::window::startup::View::Grid
            && matches!(action, "extendLeft" | "extendRight")
        {
            return KeyRun::Kept;
        }
        let busy = host.pane.busy();
        // Search keeps its unavailable listing controls silent, even while a query is in flight.
        if matches!(action, "filter" | "sortNext" | "sortReverse")
            && self.search_action(action, window, cx)
        {
            return KeyRun::Ran;
        }
        if listing && action == "escape" && self.escape_cancels_arm(arm_live, cx) {
            return KeyRun::Kept;
        }
        if matches!(action, "focusNext" | "focusPrevious")
            && self.dual_tab(action == "focusNext", window, cx)
        {
            return KeyRun::Ran;
        }
        // A listing out swallows every row action, modified or not, but the answered list.
        if matches!(action, "focusNext" | "focusPrevious") {
            self.input_action(action, window, cx);
            return KeyRun::Ran;
        }
        if self.keyboard.context == flea::keys::Context::Listing && requests::swallows(busy, action)
        {
            self.status_note(&self.locale.catalog.tr("pane.busy"), cx);
            self.hold_cursor_key(action);
            return KeyRun::Busy;
        }
        // A standing filter takes its keys ahead of the key-only handlers; the listing family runs them.
        if !self.filter_claims(action, cx) && self.key_only_action(action, window, cx) {
            return KeyRun::Ran;
        }
        let context = self.keyboard.context;
        let event = ActionEvent {
            action,
            seat,
            context,
            pair_armed,
        };
        if self.dispatch_claimed(Order::Action(event), window, cx) {
            KeyRun::Ran
        } else {
            KeyRun::Unclaimed
        }
    }
}
