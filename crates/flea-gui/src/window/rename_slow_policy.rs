//! SlowClick.js: a second plain tap on the sole marked name, after the double-click interval, renames it.
use flea::keys;

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
