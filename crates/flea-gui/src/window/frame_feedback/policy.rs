//! One per-pane boundary for terminal coverage and legacy ineligible Loading timing.
use super::{PaneFrame, Phase, Shell};
use crate::window::pane::PaneHost;
use gpui::{App, Context, Window};
use std::rc::Rc;

impl Shell {
    pub(crate) fn feedback_eligible(&self, side: usize, window: &Window, cx: &App) -> bool {
        self.lifecycle.phase() == Phase::Open
            && window.is_visible()
            && matches!(
                self.shown_view(),
                crate::window::startup::View::List | crate::window::startup::View::Dual
            )
            && self.menu.captured.is_none()
            && !self.card_holds_keyboard(cx)
            && self.slots.covers[side].is_none()
            && self
                .shown_pane(side)
                .is_some_and(|host| host.drawn_view == Some(crate::window::startup::View::List))
    }

    pub(crate) fn ineligible_loading_minimum(
        &mut self,
        side: usize,
        drawn: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let eligible = self.feedback_eligible(side, window, cx);
        if (eligible || !drawn) && self.presentation.legacy_loading[side] {
            if let Some(host) = &mut self.panes[side] {
                host.pane.swap.mark_shown(false, host.pane.now());
            }
            self.presentation.legacy_loading[side] = false;
        }
        if !drawn {
            self.presentation.legacy_loading[side] = false;
            return;
        }
        let legacy = !eligible
            && self.presentation.observer.is_some()
            && window.is_visible()
            && self.slots.covers[side].is_none()
            && self
                .shown_pane(side)
                .is_some_and(|host| host.pane.swap.mark_since.is_none());
        if legacy {
            self.pane_mark_shown(side, true, window, cx);
            self.presentation.legacy_loading[side] = true;
        }
    }
}

// A submitted pane frame still describes the live pane: same listing, search generation and observer.
pub(super) fn current(host: &PaneHost, frame: &PaneFrame) -> bool {
    let content = &frame.content;
    host.pane.identity() == content.identity
        && Rc::ptr_eq(&host.pane.held, &content.held)
        && host.pane.list_requests == content.generation
        && host.pane.swap.session.generation == content.search_generation
        && Rc::ptr_eq(&host.pane.swap.observed, &frame.observed)
}
