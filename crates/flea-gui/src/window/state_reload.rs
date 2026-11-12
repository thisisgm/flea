//! The waiting state's button and keyboard Reload share one navigation owner.
use crate::{
    g2::status_feedback::state::Reload, input::focus::Stop, pane::states::Kind, state::Shell,
};
use gpui::{Context, FocusHandle, Window};

pub(crate) const STOPS: [&str; 2] = ["reload-left", "reload-right"];

pub(super) fn requested(host: &mut super::pane::PaneHost) {
    // A newer destination must not inherit the old waiting surface during its hold interval.
    if host.pane.state.kind == Kind::Waiting {
        host.pane.state.kind = Kind::Loading;
    }
    host.waiting = false;
}

impl Shell {
    pub(super) fn show_waiting_place(&mut self, side: usize, cx: &mut Context<Self>) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let before = host.pane.path().to_owned();
        host.pane.show_waiting();
        host.waiting = true;
        host.changed(&self.locale);
        host.status = self.locale.catalog.tr("window.waiting").into();
        if host.path != before {
            host.nav.watch.clear();
        }
        self.path_labels[side] = host.path.clone().into();
        self.surfaces[side].filesystem.show_waiting(&host.path);
        cx.notify();
    }

    pub(crate) fn reload_folder(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = &self.panes[side] else {
            return;
        };
        if host.search.owner.active() {
            return;
        }
        let step = if host.pane.busy() && host.waiting {
            super::navigate::Step::Retry
        } else {
            super::navigate::Step::Reload
        };
        let _ = self.step(side, step, window, cx);
    }

    pub(crate) fn state_reload_focus(&self, side: usize, cx: &gpui::App) -> Option<FocusHandle> {
        let host = self.panes[side].as_ref()?;
        if host.pane.state.kind != Kind::Waiting || self.shown_pane(side).is_none() {
            return None;
        }
        let notice = self.surfaces[side].notice()?.read(cx);
        notice.facts().filter(|facts| facts.reload.is_some())?;
        // The handle is created when the waiting surface mounts, before the ring can visit it.
        self.input.handle(STOPS[side])
    }

    pub(crate) fn sync_state_reload(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = STOPS[side];
        let notice = self.surfaces[side].notice().cloned();
        let visible = self.shown_pane(side).is_some()
            && self.panes[side]
                .as_ref()
                .is_some_and(|host| host.waiting && host.pane.state.kind == Kind::Waiting)
            && notice.as_ref().is_some_and(|notice| {
                notice
                    .read(cx)
                    .facts()
                    .is_some_and(|facts| facts.reload.is_some())
            });
        if !visible {
            if let Some(focus) = self.input.handle(name) {
                if focus.is_focused(window) {
                    self.focus_pane(side, cx);
                    window.focus(&self.overlay_return_focus(cx), cx);
                    self.keyboard.context = flea::keys::Context::Listing;
                }
            }
            self.input.remove(name);
            return;
        }
        let Some(notice) = notice else { return };
        if self.surfaces[side].reload_watch.is_none() {
            self.surfaces[side].reload_watch =
                Some(
                    cx.subscribe_in(&notice, window, move |shell, _, _: &Reload, window, cx| {
                        // A queued click from a removed state never becomes a reload of its replacement.
                        if shell.panes[side].as_ref().is_some_and(|host| {
                            host.pane.state.kind == Kind::Waiting && host.waiting
                        }) {
                            shell.focus_pane(side, cx);
                            shell.reload_folder(side, window, cx);
                        }
                    }),
                );
        }
        let focus = notice.update(cx, |notice, cx| {
            notice.set_corner(self.corner, cx);
            notice.reload_focus(cx)
        });
        self.input.register(Stop {
            name,
            handle: focus,
            context: flea::keys::Context::Listing,
        });
    }
}
