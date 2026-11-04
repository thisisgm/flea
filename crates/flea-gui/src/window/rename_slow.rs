//! The slow click: SlowClick.js, a second lone tap on the name of the sole selected row opens its rename a double-click interval later.
use super::Shell;
use gpui::{Context, Modifiers, Task, Window};
use std::time::{Duration, Instant};

#[path = "rename_slow_policy.rs"]
pub mod policy;
use policy::{SlowClick, SlowGate};

// Qt's mouseDoubleClickInterval and the platform's double click both use 400 ms.
const INTERVAL: Duration = Duration::from_millis(policy::DOUBLE_CLICK_MS);

/// The last lone tap on a name, and the timer a second slow tap started.
pub(crate) struct Slow {
    side: usize,
    listing: usize,
    epoch: Instant,
    policy: SlowClick,
    timer: Option<Task<()>>,
}

#[cfg(test)]
impl Slow {
    pub(crate) fn pending(&self) -> bool {
        self.timer.is_some()
    }

    pub(crate) fn row(&self) -> Option<usize> {
        usize::try_from(self.policy.index).ok()
    }
}

/// One left tap's facts, read before the tap selected: its row, whether it hit the drawn name, and whether the row was already the sole selection.
pub(crate) struct Tap {
    pub row: usize,
    pub on_name: bool,
    pub modifiers: Modifiers,
    pub was_sole: bool,
}

impl super::Join {
    /// Pane.qml pressSlowClick: a press stops the timer and keeps the tap's record.
    pub(crate) fn slow_press(&mut self) {
        if let Some(slow) = self.slow.as_mut() {
            slow.timer = None;
        }
    }
}

impl Shell {
    /// The settings and pane state SlowClick.arm and fire both require.
    fn slow_gate(&self, side: usize) -> SlowGate {
        let data = &self.launch.settings.data;
        let flag = |key: &str| data.get(key).and_then(flea::jsondoc::Json::as_bool);
        let host = self.panes[side].as_ref();
        SlowGate {
            single_click: self.launch.settings.single_click_open(),
            click_rename: flag("clickRename") != Some(false),
            renaming: self.rename.edit.is_some(),
            search: host.is_none_or(|host| host.pane.state.search),
            band: host.is_some_and(|host| host.input.band.is_some()),
            ..SlowGate::default()
        }
    }

    /// Tap.slowTap: a lone tap on the name records itself, and a second one past the interval starts the timer.
    pub(crate) fn slow_tap(
        &mut self,
        side: usize,
        tap: Tap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Tap {
            row,
            on_name,
            modifiers,
            was_sole,
        } = tap;
        let Some(listing) = self.panes[side].as_ref().map(|host| host.pane.held.listing) else {
            return;
        };
        if !on_name {
            self.rename.slow = None;
            return;
        }
        let now = cx.background_executor().now();
        let prior = self
            .rename
            .slow
            .take()
            .filter(|prior| (prior.side, prior.listing) == (side, listing));
        let (mut policy, epoch) = prior.map_or((SlowClick::default(), now), |prior| {
            (prior.policy, prior.epoch)
        });
        // The shared arithmetic uses caller units; nanoseconds preserve Instant's interval boundary.
        let elapsed =
            u64::try_from(now.saturating_duration_since(epoch).as_nanos()).unwrap_or(u64::MAX);
        let armed = policy.arm(
            row as i64,
            crate::window::input::modifiers(modifiers),
            elapsed,
            INTERVAL.as_nanos() as u64,
            self.slow_gate(side),
            was_sole,
        );
        self.rename.slow = Some(Slow {
            side,
            listing,
            epoch,
            policy,
            timer: None,
        });
        if !armed {
            return;
        }
        let timer = cx.spawn_in(window, async move |shell, cx| {
            cx.background_executor().timer(INTERVAL).await;
            let _ =
                cx.update(|window, cx| shell.update(cx, |shell, cx| shell.slow_fire(window, cx)));
        });
        if let Some(slow) = self.rename.slow.as_mut() {
            slow.timer = Some(timer);
        }
    }

    /// Pane.qml cancelSlowClick: every dispatched action ends a pending slow click.
    pub(crate) fn slow_cancel(&mut self) {
        self.rename.slow = None;
    }

    /// SlowClick.fire: renames only when the armed row is still the cursor and the one selected row.
    fn slow_fire(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut slow) = self.rename.slow.take() else {
            return;
        };
        let side = slow.side;
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        if host.pane.held.listing != slow.listing || host.input.press.is_some() {
            return;
        }
        let gate = SlowGate {
            menu: self.menu_opened(),
            ..self.slow_gate(side)
        };
        if slow
            .policy
            .fire(gate, &host.selected, host.pane.cursor)
            .is_none()
        {
            return;
        }
        self.rename_open(side, 0, window, cx);
        self.rename_sync_gate(window, cx);
    }

    /// A press outside the editor commits it first (Tap.js), except on the tab strip, which never commits (Tabs.js tearRefusal reads the editor).
    pub(crate) fn rename_press_away(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .strip
            .bounds
            .get()
            .is_some_and(|strip| strip.contains(&at))
        {
            return;
        }
        self.rename_commit(cx);
        self.rename_sync_gate(window, cx);
    }
}
