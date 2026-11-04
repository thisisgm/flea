//! A selection wider than the held rows lifts by index: the source backend names its paths, and a drop made first waits for them.
use super::*;

// One paths ask a window's lift owes on its source side, matched to the lift by serial.
pub(super) struct Ask {
    serial: u64,
    listing: usize,
}

/// Where a released lift lands: the pane spot under a point, a resolved folder, or the rail's Trash.
pub(super) enum Then {
    Tab(super::tab_drop::Destination),
    Into(String),
    Trash,
}

// A release that came before the paths answer: it lands once the paths arrive.
pub(super) struct Waiting {
    side: usize,
    path: Option<String>,
    then: Then,
    owned: Owned,
}

impl Shell {
    /// Ask the source backend for the lift's paths, unless another asker holds this side's paths slot.
    pub(super) fn ask_lift_paths(
        &mut self,
        side: usize,
        lift: &Lift,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let taken = self.clipboard.board.paths_pending(side as u64)
            || self.carry_pending(side, window, cx)
            || self.opener[side].waiting();
        let Some(host) = self.panes[side].as_mut() else {
            return false;
        };
        if taken || host.nav.gate.paths_pending || host.nav.gate.clip_pending {
            return false;
        }
        let line = flea::ui_logic_dg1::paths_request_line(&lift.rows);
        if !host
            .backend
            .as_ref()
            .is_some_and(|backend| backend.line(line).is_ok())
        {
            return false;
        }
        host.nav.gate.clip_pending = true;
        cx.default_global::<Drags>().asks.insert(
            (window_key(window), side),
            Ask {
                serial: lift.serial,
                listing: lift.listing,
            },
        );
        true
    }

    /// A released lift lands now when its paths are known, else waits for them or says it could not resolve.
    pub(super) fn land_then(
        &mut self,
        side: usize,
        then: Then,
        owned: Owned,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = window_key(window);
        let Owned::Own { lift, ctrl, shift } = &owned else {
            return self.land_now(side, then, owned, window, cx);
        };
        if lift.paths.is_some() {
            return self.land_now(side, then, owned, window, cx);
        }
        let (serial, source, count, ctrl, shift) =
            (lift.serial, lift.side, lift.rows.len(), *ctrl, *shift);
        let drags = cx.default_global::<Drags>();
        if drags
            .asks
            .get(&(key, source))
            .is_some_and(|ask| ask.serial == serial)
        {
            let Some(host) = self.panes[side].as_ref() else {
                return;
            };
            let path = (!matches!(&then, Then::Tab(_))).then(|| host.path.clone());
            drags.waiting.insert(
                (key, source),
                Waiting {
                    side,
                    path,
                    then,
                    owned,
                },
            );
            return;
        }
        let text = core::too_wide(&self.locale.catalog, count, ctrl, shift);
        self.say_drop(source, text, cx);
    }

    fn land_now(
        &mut self,
        side: usize,
        then: Then,
        owned: Owned,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.input_blocked(cx) {
            return;
        }
        match (then, owned) {
            (Then::Tab(tab), owned) => self.land_file_tab(tab, owned, window, cx),
            (Then::Into(dest), owned) => {
                if self.panes[side]
                    .as_ref()
                    .is_some_and(|host| host.pane.busy())
                {
                    self.refuse_drop_loading(side, cx);
                } else {
                    self.land_into(side, dest, owned, window, cx);
                }
            }
            (Then::Trash, Owned::Own { lift, .. }) => self.trash_lift(&lift, cx),
            (Then::Trash, Owned::Native(_)) => {}
        }
    }

    /// The paths answer to this window's lift ask: it completes the lift, and lands a release that waited for it.
    pub(crate) fn drag_paths(
        &mut self,
        side: usize,
        paths: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = window_key(window);
        let Some(ask) = self.take_lift_ask(side, window, cx) else {
            return false;
        };
        // The backend names rows of its current listing, so a listing changed since the lift resolves nothing.
        let current = self.panes[side]
            .as_ref()
            .is_some_and(|host| host.pane.held.listing == ask.listing);
        let fits =
            |lift: &Lift| current && lift.serial == ask.serial && lift.rows.len() == paths.len();
        let drags = cx.global_mut::<Drags>();
        if let Some(lift) = drags.lifts.get_mut(&key).filter(|lift| fits(lift)) {
            lift.paths = Some(paths.to_vec());
        }
        if let Some(mut waiting) = drags.waiting.remove(&(key, side)) {
            if let Owned::Own { lift, ctrl, shift } = &mut waiting.owned {
                if !fits(lift) {
                    let text = core::too_wide(&self.locale.catalog, lift.rows.len(), *ctrl, *shift);
                    self.say_drop(side, text, cx);
                    return true;
                }
                lift.paths = Some(paths.to_vec());
            }
            if waiting.path.as_ref().is_some_and(|path| {
                self.panes[waiting.side]
                    .as_ref()
                    .is_none_or(|host| &host.path != path)
            }) {
                return true;
            }
            self.land_now(waiting.side, waiting.then, waiting.owned, window, cx);
        }
        cx.notify();
        true
    }

    /// A refused paths ask ends the lift's wait with its line.
    pub(crate) fn drag_paths_failed(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.take_lift_ask(side, window, cx).is_none() {
            return false;
        }
        if let Some(Waiting {
            owned: Owned::Own { lift, ctrl, shift },
            ..
        }) = cx
            .global_mut::<Drags>()
            .waiting
            .remove(&(window_key(window), side))
        {
            let text = core::too_wide(&self.locale.catalog, lift.rows.len(), ctrl, shift);
            self.say_drop(side, text, cx);
        }
        true
    }

    // Claim the ask this side owes and hand the paths slot back to any other asker still waiting on it.
    fn take_lift_ask(
        &mut self,
        side: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Ask> {
        let key = (window_key(window), side);
        let drags = cx.try_global::<Drags>()?;
        drags.asks.get(&key)?;
        let ask = cx.global_mut::<Drags>().asks.remove(&key)?;
        let pending =
            self.clipboard.board.paths_pending(side as u64) || self.carry_pending(side, window, cx);
        if let Some(host) = self.panes[side].as_mut() {
            host.nav.gate.clip_pending = pending;
        }
        Some(ask)
    }
}
