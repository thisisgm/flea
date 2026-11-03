//! The flash view covers its pane's slot as Trash does; a navigation on that pane leaves it, as Pane.qml's open does.
use super::*;
use crate::{navigation::Refusal, window::navigate::Step, window::startup::View};
use gpui::App;

impl Shell {
    /// The flash view stands over this side's pane, so the pane draws nothing under it.
    pub(crate) fn flash_covers(&self, side: usize) -> bool {
        let archive = &self.operations.archive;
        archive.flash.shown()
            && archive.flash_side == side
            && (side == 0 || self.view == View::Dual)
    }

    /// The flash pane holds the keys while its own pane is drawn and has them, never while the rail or the other pane does.
    pub(crate) fn flash_has_keys(&self, window: &Window, cx: &App) -> bool {
        let side = self.operations.archive.flash_side;
        self.flash_covers(side)
            && matches!(self.focus_view(window, cx), "list" | "")
            && (self.view != View::Dual || self.focus_side == side)
    }

    /// FlashHost.qml onFocusRailRequested: Tab from the flash pane hands the keys to the rail.
    pub(crate) fn flash_to_rail(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(rail) = self
            .input
            .step("list", true)
            .filter(|stop| stop.name == "rail")
        else {
            return false;
        };
        let (handle, context) = (rail.handle.clone(), rail.context);
        window.focus(&handle, cx);
        self.keyboard.context = context;
        cx.notify();
        true
    }

    /// A press anywhere on the flash pane gives it the keys back, as a press on a pane focuses its list.
    pub(crate) fn flash_take_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.flash_has_keys(window, cx) {
            return;
        }
        self.focus_side = self.operations.archive.flash_side;
        self.rail_focus(false, cx);
        window.focus(&self.focus, cx);
        self.keyboard.context = flea::keys::Context::Listing;
        cx.notify();
    }

    /// Pane.qml goBack and openParent leave the covering place and stop; open leaves it and goes on.
    pub(crate) fn covered_step(
        &mut self,
        side: usize,
        step: &Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Result<bool, Refusal>> {
        if matches!(
            step,
            Step::Open(_)
                | Step::OpenPlace(_)
                | Step::Back
                | Step::MouseBack
                | Step::Up
                | Step::Forward
        ) {
            let shares = self
                .rail
                .as_ref()
                .and_then(|join| join.network.as_ref())
                .map(|network| network.shares.clone())
                .filter(|shares| {
                    let browser = &shares.read(cx).model;
                    browser.open && self.network_origin_side(browser.origin) == Some(side)
                });
            if let Some(shares) = shares {
                shares.update(cx, |shares, cx| shares.close(cx));
            }
        }
        if let Some(host) = self
            .trash
            .as_ref()
            .filter(|host| host.side == side && host.pane.read(cx).view().opened)
        {
            let stops = match step {
                Step::Back | Step::MouseBack | Step::Up => true,
                Step::Forward => return Some(Ok(false)),
                Step::Open(_) | Step::OpenCursor | Step::OpenPlace(_) => false,
                _ => return self.recent_step(side, step, window, cx),
            };
            if host.pane.read(cx).view().confirming {
                return Some(Ok(false));
            }
            self.drop_trash(side, window, cx);
            if stops {
                return Some(Ok(true));
            }
        }
        if let Some(done) = self.plugin_step(side, step, window, cx) {
            return Some(done);
        }
        if self.flash_covers(side) {
            let stops = match step {
                Step::Back | Step::MouseBack | Step::Up => true,
                Step::Forward => return Some(Ok(false)),
                Step::Open(_) | Step::OpenCursor | Step::OpenPlace(_) => false,
                _ => return self.recent_step(side, step, window, cx),
            };
            let effects = self.operations.archive.flash.close();
            self.archive_effects(side, effects, window, cx);
            if stops {
                cx.notify();
                return Some(Ok(true));
            }
        }
        self.recent_step(side, step, window, cx)
    }
}
