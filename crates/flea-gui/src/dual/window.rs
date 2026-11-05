//! Live dual session and divider operations.
use crate::{state::Shell, window::startup::View};
use gpui::{Context, Window};

impl Shell {
    /// WindowBody.rememberDual: the focus save carries focus and paths, leaves the split alone, and saves nothing outside Dual or when unchanged (ViewState.owe).
    pub(crate) fn remember_dual(&mut self) {
        if self.view != View::Dual {
            return;
        }
        let paths = match (&self.panes[0], &self.panes[1]) {
            (Some(left), Some(right)) => Some((left.path.clone(), right.path.clone())),
            _ => None,
        };
        let patch = crate::dual::session::remember_patch(
            self.focus_side,
            paths
                .as_ref()
                .map(|paths| (paths.0.as_str(), paths.1.as_str())),
        );
        if self.dual_written.as_deref() == Some(patch.as_str()) {
            return;
        }
        let failure = self.locale.catalog.tr("dual.remember_failed");
        if let Some(host) = self.panes.iter().flatten().next() {
            host.nav.writer.borrow_mut().write(patch.clone(), failure);
            self.dual_written = Some(patch);
        }
    }

    /// One split leaf on divider release; the layout re-lays from it.
    pub(crate) fn set_split(&mut self, split: i64) {
        self.launch.settings.split = split as f32 / crate::dual::SCALE as f32;
        let patch = crate::dual::session::split_patch(split);
        let failure = self.locale.catalog.tr("dual.remember_failed");
        if let Some(host) = self.panes.iter().flatten().next() {
            host.nav.writer.borrow_mut().write(patch, failure);
        }
    }

    pub(crate) fn dual_min(&self) -> f32 {
        self.panes[0]
            .as_ref()
            .map_or(crate::dual::ROOM_MIN, |host| {
                let list = host.list.borrow();
                let metrics = crate::list::Metrics::at(list.typeface.body);
                2. * metrics.pad
                    + 1.45 * f32::from(list.typeface.body_small)
                    + metrics.gap
                    + 180. * f32::from(list.typeface.body_small) / 13.
            })
    }

    pub(crate) fn dual_room(&self, window: &Window) -> (f32, f32) {
        let geometry = self.geometry(window);
        let room = geometry.panes.iter().map(|pane| pane.width).sum();
        (room, geometry.panes[0].width)
    }

    /// The divider press opens a drag; a double click restores the equal split.
    pub(crate) fn dual_begin(
        &mut self,
        x: f32,
        clicks: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.view != View::Dual {
            return;
        }
        if clicks >= 2 {
            self.dual_drag = None;
            self.set_split(crate::dual::EQUAL_SPLIT);
            cx.notify();
            return;
        }
        let (_, left) = self.dual_room(window);
        self.dual_drag = Some(crate::dual::Drag::begin(x, left));
    }

    /// The accent line follows the pointer; the listings re-lay on release.
    pub(crate) fn dual_move(&mut self, x: f32, window: &Window, cx: &mut Context<Self>) {
        let (room, _) = self.dual_room(window);
        let min = self.dual_min();
        let Some(drag) = self.dual_drag.as_mut() else {
            return;
        };
        drag.travel_to(x, room, min);
        cx.notify();
    }

    /// Deactivation cancels a live drag whatever holds focus, as a stolen grab; new() and a test share this arm.
    pub(crate) fn subscribe_blur(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dual_blur = Some(cx.observe_window_activation(window, |shell, window, cx| {
            if !window.is_window_active() {
                crate::g2::pv2::selection::blur(window, cx);
                if let Some(edit) = shell.rename.edit.as_mut() {
                    edit.line.end_selection_drag();
                }
                shell.dual_cancel(cx);
                for side in 0..2 {
                    shell.cancel_pointer(side, window, cx);
                }
            }
        }));
    }

    /// The stolen grab resets the drag with no write, as onCanceled does before any later release can commit.
    pub(crate) fn dual_cancel(&mut self, cx: &mut Context<Self>) {
        self.dual_hover = false;
        if self.dual_drag.take().is_some() {
            cx.notify();
        }
    }

    /// One leaf on release, so both panes re-lay from it in the same frame.
    pub(crate) fn dual_end(&mut self, commit: bool, window: &Window, cx: &mut Context<Self>) {
        let Some(drag) = self.dual_drag.take() else {
            return;
        };
        if commit {
            let (room, _) = self.dual_room(window);
            if let Some(split) = drag.finish(room, self.dual_min()) {
                self.set_split(split);
            }
        }
        cx.notify();
    }
}
