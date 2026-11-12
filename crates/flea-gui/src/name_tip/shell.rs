//! The window and chooser share the overlay; only currently mounted row painters are eligible.
use super::{columns, list_candidate, Candidate, Overlay, Owner};
use crate::{state::Shell, window::startup::View};
use gpui::{deferred, prelude::*, App, Context, Div, Entity, Window};
use std::rc::Rc;

impl Shell {
    pub(crate) fn name_tips(&self, body: Div, cx: &Context<Self>) -> Div {
        let shell = cx.entity().downgrade();
        let overlay = Overlay::new(self.name_tip.clone(), cx.entity_id(), move |window, cx| {
            shell.upgrade()?.read(cx).name_tip_candidate(window, cx)
        });
        super::key_events(
            body.child(deferred(overlay)),
            self.name_tip.clone(),
            cx.entity_id(),
        )
    }

    pub(super) fn name_tip_candidate(&self, window: &Window, cx: &App) -> Option<Candidate> {
        if self.input_blocked(cx)
            || self.carry_riding_or_pending(cx)
            || (0..2).any(|side| self.renaming_row(side).is_some())
        {
            return None;
        }
        let at = window.mouse_position();
        let view = self.shown_view();
        let right = window.viewport_size().width
            - gpui::px(crate::list::Metrics::for_typeface(&self.active_typeface()).pad);
        for (side, host) in self.panes.iter().enumerate() {
            let Some(host) = host else { continue };
            if (side == 1 && view != View::Dual)
                || host.pane.busy()
                || self.slots.covers[side].is_some()
                || self.flash_covers(side)
            {
                continue;
            }
            if host.drawn_list && matches!(view, View::List | View::Dual) {
                if let Some(mut candidate) = list_candidate(
                    &host.list.borrow(),
                    at,
                    host.pane.held.listing,
                    right,
                    Rc::as_ptr(&host.list) as usize,
                    view == View::Dual && side != self.focus_side,
                ) {
                    candidate.epoch = host.pane.swap.session.generation;
                    return Some(candidate);
                }
            }
            if view != View::Columns || host.drawn_view != Some(View::Columns) {
                continue;
            }
            let Some(columns) = host.columns.as_ref() else {
                continue;
            };
            for (index, track) in columns.area.tracks().iter().enumerate() {
                let Some(paint) = columns.painters.get(track.slot) else {
                    continue;
                };
                let paint_ref = paint.borrow();
                if !paint_ref.drawn.as_ref().is_some_and(|drawn| {
                    Rc::ptr_eq(&drawn.tracks, columns.area.tracks()) && drawn.at == index
                }) {
                    continue;
                }
                if let Some(mut candidate) = columns::candidate(
                    &paint_ref,
                    at,
                    host.pane.held.listing,
                    right,
                    Rc::as_ptr(paint) as usize,
                ) {
                    candidate.epoch = host.pane.swap.session.generation;
                    return Some(candidate);
                }
            }
        }
        None
    }
}

pub(crate) fn picker(
    view: Entity<crate::g2::picker_listing::view::ListingView>,
    owner: Owner,
) -> Overlay {
    let id = view.entity_id();
    let view = view.downgrade();
    Overlay::new(owner, id, move |window, cx| {
        let view = view.upgrade()?;
        let view = view.read(cx);
        if view.view_mode() != "list" || view.listing().state() != "ready" || view.name_tips_blocked
        {
            return None;
        }
        let state = view.name_tip_state();
        let state = state.borrow();
        let right = (state.drawn.bounds.right() - gpui::px(state.drawn.metrics.lane))
            .min(window.viewport_size().width - gpui::px(state.drawn.metrics.pad));
        list_candidate(
            &state,
            window.mouse_position(),
            view.listing().held().listing,
            right,
            Rc::as_ptr(view.name_tip_state()) as usize,
            false,
        )
    })
}

pub(crate) fn picker_keys<E: InteractiveElement>(
    root: E,
    view: &Entity<crate::g2::picker_listing::view::ListingView>,
    cx: &App,
) -> E {
    super::key_events(root, view.read(cx).name_tip.clone(), view.entity_id())
}
