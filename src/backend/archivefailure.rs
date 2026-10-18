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

// Issue #211: stderr's last non-blank line diagnoses a bad archive; otherwise the status names the signal.
pub(crate) fn failure_message(what: &str, status: &ExitStatus, stderr: &str) -> String {
    // A blank line is not a diagnosis.
    if let Some(line) = stderr.lines().rev().find(|line| !line.trim().is_empty()) {
        return line.trim().to_string();
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

