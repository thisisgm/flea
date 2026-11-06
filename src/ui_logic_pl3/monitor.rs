//! TrashMonitor.qml: GIO's shared Trash count, re-read after a settle once its watch reports a change.

/// A burst of monitor lines settles this long before one count read.
pub const SETTLE_MS: u64 = 120;
/// A count read that has not finished by now is abandoned as failed.
pub const TIMEOUT_MS: u64 = 10_000;

pub const WATCH: [&str; 3] = ["gio", "monitor", "--dir=trash:///"];
pub const QUERY: [&str; 4] = ["gio", "info", "--attributes=trash::item-count", "trash:///"];

/// setpriv ties the helper to this UI, and the PPID check covers a death before the signal is registered.
pub fn owned_command(ui_pid: u32, args: &[&str]) -> Vec<String> {
    let mut command: Vec<String> = ["setpriv", "--pdeathsig", "TERM", "--", "sh", "-c", r#"[ "$PPID" = "$1" ] || exit 1; shift; exec "$@""#, "flea-trash"]
        .iter().map(|s| s.to_string()).collect();
    command.push(ui_pid.to_string());
    command.extend(args.iter().map(|s| s.to_string()));
    command
}

/// Sample GIO output: "  trash::item-count: 12".
pub fn parse_count(text: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let value = line.trim().strip_prefix("trash::item-count:")?.trim();
        (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())).then(|| value.parse().ok())?
    })
}

/// One count read at a time; a refresh asked for meanwhile runs once after it.
#[derive(Debug, Default)]
pub struct Count {
    pub count: u64,
    pub ready: bool,
    collecting: bool,
    pending: bool,
}

/// What finishing a read decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Read {
    pub failed: bool,
    pub again: bool,
}

impl Count {
    /// True when the caller should start a read now.
    pub fn refresh(&mut self) -> bool {
        if self.collecting {
            self.pending = true;
            return false;
        }
        self.pending = false;
        self.collecting = true;
        true
    }

    /// The read's stdout once both its stream finished and the process exited.
    pub fn finished(&mut self, exit_code: i32, timed_out: bool, output: &str) -> Read {
        self.collecting = false;
        let failed = match (exit_code == 0 && !timed_out).then(|| parse_count(output)).flatten() {
            Some(count) => {
                self.count = count;
                self.ready = true;
                false
            }
            None => true,
        };
        let again = std::mem::take(&mut self.pending);
        Read { failed, again }
    }

    /// Watching stopped: nothing pending survives.
    pub fn stop(&mut self) {
        self.collecting = false;
        self.pending = false;
    }
}

/// Debounce for a same-count `Changed` soft re-list while Trash is open and idle.
/// Longer than [`SETTLE_MS`] so a settled monitor burst coalesces; cancelled on scroll.
pub const SOFT_RELIST_MS: u64 = 500;

/// A GIO trash-watch event that may refresh an open Trash view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relist {
    /// Settled burst; an open delete card re-reviews immediately.
    /// A plain open listing may soft-debounce a re-list outside this helper.
    Changed,
    /// Fresh item count from GIO.
    Count(usize),
}

/// Whether an open Trash listing should re-list *immediately* for this watch event.
///
/// GIO's trash watch can also fire while we page the Trash window during scroll. A full
/// re-list on every `Changed` looks like Trash keeps re-reading. Real content changes
/// usually move the item count; an open delete card still re-reviews on `Changed`.
/// Same-count membership swaps are not caught here — the host schedules a soft
/// debounced re-list on `Changed` when not confirming and not scrolling.
pub fn should_relist(opened: bool, confirming: bool, listed_total: usize, event: Relist) -> bool {
    match event {
        Relist::Changed => confirming,
        // Count delta re-lists even while confirming so an open card re-reviews.
        Relist::Count(total) => opened && listed_total != total,
    }
}

#[cfg(test)]
mod should_relist_tests {
    use super::{should_relist, Relist};

    #[test]
    fn scroll_noise_does_not_relist() {
        assert!(!should_relist(true, false, 51, Relist::Changed));
        assert!(!should_relist(true, false, 51, Relist::Count(51)));
    }

    #[test]
    fn a_new_count_relists() {
        assert!(should_relist(true, false, 51, Relist::Count(50)));
        assert!(!should_relist(false, false, 0, Relist::Count(50)));
    }

    #[test]
    fn an_open_card_rereviews_on_changed() {
        assert!(should_relist(true, true, 51, Relist::Changed));
        assert!(should_relist(true, true, 51, Relist::Count(50)));
        assert!(!should_relist(true, true, 51, Relist::Count(51)));
    }
}
