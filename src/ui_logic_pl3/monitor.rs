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
