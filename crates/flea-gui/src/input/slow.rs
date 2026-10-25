//! SlowClick.js: a second plain tap on the sole marked name, after the double-click interval, renames it.
use crate::{
    state::Shell,
    window::{dispatch::Order, input::ActionEvent},
};
use flea::keys;
use gpui::{Context, Window};
use std::time::Duration;

/// The platform's double-click interval, which the slow click waits out before it may rename.
pub const DOUBLE_CLICK_MS: u64 = 400;
/// The cleared slow-click index, so no prior tap can match.
pub const CLEARED: i64 = -2;

/// What gates a slow-click rename, read at arm time and again when its timer fires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlowGate {
    pub single_click: bool,
    pub click_rename: bool,
    pub search: bool,
    pub renaming: bool,
    pub band: bool,
    pub drag: bool,
    pub menu: bool,
}

/// The tap the view captured before it selected: the sole mark is the cursor's row.
pub fn was_sole_selection(count: usize, selected: bool, cursor_here: bool) -> bool {
    count == 1 && selected && cursor_here
}

/// The last tap's row and time; a tap arms the pane timer, a double click cancels, the timer renames a still-held sole row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlowClick {
    pub at: u64,
    pub index: i64,
}

impl Default for SlowClick {
    fn default() -> Self {
        Self {
            at: 0,
            index: CLEARED,
        }
    }
}

impl SlowClick {
    /// Whether this tap starts the timer; every tap is recorded so a tap on another row re-arms.
    pub fn arm(
        &mut self,
        index: i64,
        modifiers: u8,
        now: u64,
        interval: u64,
        gate: SlowGate,
        was_sole: bool,
    ) -> bool {
        let plain = modifiers & (keys::CONTROL | keys::SHIFT) == 0;
        let (prior_at, prior) = (self.at, self.index);
        self.at = now;
        self.index = index;
        let open = index >= 0
            && plain
            && !gate.single_click
            && gate.click_rename
            && !gate.search
            && !gate.renaming
            && !gate.band
            && !gate.drag;
        open && was_sole && prior == index && now.saturating_sub(prior_at) > interval
    }

    /// The timer firing renames only when the cursor and the sole mark still hold the armed row.
    pub fn fire(&mut self, gate: SlowGate, selected: &[usize], cursor: usize) -> Option<usize> {
        let index = std::mem::replace(&mut self.index, CLEARED);
        let index = usize::try_from(index).ok()?;
        let blocked = gate.menu
            || gate.drag
            || gate.renaming
            || gate.single_click
            || !gate.click_rename
            || gate.search
            || gate.band;
        (!blocked && selected == [index] && cursor == index).then_some(index)
    }

    pub fn cancel(&mut self) {
        self.index = CLEARED;
    }
}

/// One released left tap: its row, whether it landed on the drawn name, its modifiers, and the mark it found.
pub(crate) struct SlowTap {
    pub row: usize,
    pub armed: bool,
    pub modifiers: u8,
    pub was_sole: bool,
}

impl Shell {
    /// The gate as the pane stands now: the settings, a search, an open or pending rename, a band, a live press, a menu.
    fn slow_gate(&self, side: usize, cx: &gpui::App) -> Option<SlowGate> {
        let host = self.panes[side].as_ref()?;
        let settings = &self.launch.settings.data;
        let gate = host.nav.gate;
        Some(SlowGate {
            single_click: settings.get("openMode").and_then(flea::jsondoc::Json::as_str)
                == Some("single"),
            click_rename: settings
                .get("clickRename")
                .and_then(flea::jsondoc::Json::as_bool)
                != Some(false),
            search: host.pane.state.search || gate.search,
            renaming: gate.editing || gate.rename_pending,
            band: host.input.band.as_ref().is_some_and(|band| band.active),
            drag: host.input.press.as_ref().is_some_and(|press| press.dragged),
            menu: self.input_blocked(cx),
        })
    }

    /// Tap.slowTap: a single tap on the drawn name arms the timer, any other tap cancels it.
    pub(crate) fn slow_tap(
        &mut self,
        side: usize,
        tap: SlowTap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let SlowTap {
            row,
            armed,
            modifiers,
            was_sole,
        } = tap;
        let Some(gate) = self.slow_gate(side, cx) else {
            return;
        };
        let now = cx.background_executor().now();
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        let now = host.input.ms(now) as u64;
        if !armed {
            host.input.slow.cancel();
            host.input.slow_task = None;
            return;
        }
        if !host
            .input
            .slow
            .arm(row as i64, modifiers, now, DOUBLE_CLICK_MS, gate, was_sole)
        {
            host.input.slow_task = None;
            return;
        }
        host.input.slow_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(DOUBLE_CLICK_MS))
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |shell, cx| shell.slow_fired(side, window, cx))
            });
        }));
    }

    /// The timer renames only while the cursor and the sole mark still hold the armed row.
    fn slow_fired(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(gate) = self.slow_gate(side, cx) else {
            return;
        };
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        host.input.slow_task = None;
        let Some(row) = host.input.slow.fire(gate, &host.selected, host.pane.cursor) else {
            return;
        };
        if side != self.focus_side {
            return;
        }
        let event = ActionEvent {
            action: "rename",
            context: keys::Context::Listing,
            seat: keys::sequence::Seat {
                listing: host.pane.held.listing as u64,
                cursor: row,
                view: self.view as u64,
                focus: side as u64,
                ..Default::default()
            },
            pair_armed: false,
        };
        self.dispatch(Order::Action(event), window, cx);
    }

    /// Every dispatched action ends a pending slow click, so no editor follows the action's own result.
    pub(crate) fn cancel_slow_click(&mut self) {
        for host in self.panes.iter_mut().flatten() {
            host.input.slow.cancel();
            host.input.slow_task = None;
        }
    }
}
