use super::{ExitStatus, ExitStatusExt};

// The signal a job died from; anything else keeps its number rather than a guessed name.
fn signal_name(signal: i32) -> Option<&'static str> {
    match signal {
        6 => Some("SIGABRT"),
        9 => Some("SIGKILL"),
        11 => Some("SIGSEGV"),
        15 => Some("SIGTERM"),
        24 => Some("SIGXCPU"),
        _ => None,
    }
}

// bwrap renders a tool the kernel killed as exit 128+n, so this offset recovers the signal.
const BWRAP_SIGNAL_BASE: i32 = 128;

// bsdtar's generic last line after member errors, which says nothing about which member failed.
const DELAYED_ERRORS: &str = "Error exit delayed from previous errors.";

// Sample: "bsdtar: Ignoring malformed pax extended attributes\nblocked/child: Could not stat blocked/child: Not a directory\nbsdtar: Error exit delayed from previous errors."
// Issue #211: the last non-blank stderr line diagnoses; under that trailer, the first line without the trailer's tool prefix; otherwise the status does.
pub(crate) fn failure_message(what: &str, status: &ExitStatus, stderr: &str) -> String {
    // A blank line is not a diagnosis.
    let mut lines = stderr.lines().map(str::trim).filter(|line| !line.is_empty());
    if let Some(first) = lines.next() {
        let last = lines.next_back().unwrap_or(first);
        let Some(tool) = last.strip_suffix(DELAYED_ERRORS) else {
            return last.to_string();
        };
        // corner: a member error whose own name starts like that prefix reads as a warning, and the first line then answers.
        let member = std::iter::once(first).chain(lines).find(|line| tool.is_empty() || !line.starts_with(tool));
        return member.unwrap_or(first).to_string();
    }
    let killed = |signal: i32| match signal_name(signal) {
        Some(name) => format!("the {} tool stopped before it finished (signal {})", what, name),
        None => format!("the {} tool stopped before it finished (signal {})", what, signal),
    };
    if let Some(signal) = status.signal() {
        return killed(signal);
    }
    match status.code() {
        // 128+n is bwrap's rendering of a signal; an unknown one stays an exit status.
        Some(code) if signal_name(code - BWRAP_SIGNAL_BASE).is_some() => killed(code - BWRAP_SIGNAL_BASE),
        Some(code) => format!("the {} tool could not finish (exit status {})", what, code),
        // Unreachable in practice; this function always answers a sentence rather than panicking.
        None => format!("the {} tool could not finish", what),
    }
}

