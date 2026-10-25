//! The slow click: SlowClick.js, a second lone tap on the name of the sole selected row opens its rename a double-click interval later.
use super::Shell;
use gpui::{Context, Modifiers, Pixels, Task, Window};
use std::time::{Duration, Instant};

// Qt's mouseDoubleClickInterval and the platform's double click both use 400 ms.
const INTERVAL: Duration = Duration::from_millis(400);

/// The last lone tap on a name, and the timer a second slow tap started.
pub(crate) struct Slow {
    side: usize,
    listing: usize,
    row: usize,
    at: Instant,
    timer: Option<Task<()>>,
}

/// One left tap's facts, read before the tap selected: its row, whether it hit the drawn name, and whether the row was already the sole selection.
pub(crate) struct Tap {
    pub row: usize,
    pub on_name: bool,
    pub modifiers: Modifiers,
    pub was_sole: bool,
}

/// Tap.onName: the press landed on the row's drawn name, not the space past it.
pub(crate) fn on_name(list: &crate::list::ListState, row: usize, x: Pixels) -> bool {
    let drawn = &list.drawn;
    let at = f32::from(x - drawn.bounds.origin.x) - drawn.layout.name.x;
    list.drawn_name_width(row)
        .is_some_and(|width| (0. ..width).contains(&at))
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
    /// The settings and the pane state SlowClick.arm and fire both require.
    fn slow_allowed(&self, side: usize) -> bool {
        let data = &self.launch.settings.data;
        let flag = |key: &str| data.get(key).and_then(flea::jsondoc::Json::as_bool);
        flag("singleClick") != Some(true)
            && flag("clickRename") != Some(false)
            && self.rename.edit.is_none()
            && self.panes[side]
                .as_ref()
                .is_some_and(|host| !host.pane.state.search && host.input.band.is_none())
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
        let prior = self.rename.slow.take();
        self.rename.slow = Some(Slow {
            side,
            listing,
            row,
            at: now,
            timer: None,
        });
        let plain = !modifiers.control && !modifiers.shift;
        let again = prior.is_some_and(|prior| {
            (prior.side, prior.listing, prior.row) == (side, listing, row)
                && now.saturating_duration_since(prior.at) > INTERVAL
        });
        if !(plain && was_sole && again && self.slow_allowed(side)) {
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
        let Some(slow) = self.rename.slow.take() else {
            return;
        };
        let side = slow.side;
        let held = self.panes[side].as_ref().is_some_and(|host| {
            host.pane.held.listing == slow.listing
                && host.selected == [slow.row]
                && host.pane.cursor == slow.row
                && host.input.press.is_none()
        });
        if !held || self.menu_opened() || !self.slow_allowed(side) {
            return;
        }
        self.rename_open(side, 0, window, cx);
        self.rename_sync_gate(window, cx);
    }
}
