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

// bsdtar's generic last line after member errors, which says nothing about which member failed; libarchive 3.7 ends it with a period, 3.8 does not.
const DELAYED_ERRORS: &str = "Error exit delayed from previous errors";
// bsdtar's warning about a header it skipped, which never says why the extract failed.
const IGNORING: &str = "Ignoring ";
// bsdtar's note that it is resyncing after damage, printed after the damage error itself.
const RETRYING: &str = "Retrying...";

// Whether the text after bsdtar's own prefix is a warning or retry note, which never says why the extract failed.
pub(crate) fn tool_note(text: &str) -> bool {
    text.starts_with(IGNORING) || text == RETRYING
}

// The text before bsdtar's delayed-error trailer when the line is that trailer, in either spelling.
pub(crate) fn before_trailer(line: &str) -> Option<&str> {
    line.strip_suffix('.').unwrap_or(line).strip_suffix(DELAYED_ERRORS)
}

// Sample: "bsdtar: Ignoring malformed pax extended attributes\nblocked/child: Could not stat blocked/child: Not a directory\nbsdtar: Error exit delayed from previous errors."
// Issue #211: the last non-blank stderr line diagnoses; under that trailer, the first line without the trailer's tool prefix, else the first prefixed line that is no warning or retry note; otherwise the status does.
pub(crate) fn failure_message(what: &str, status: &ExitStatus, stderr: &str) -> String {
    // A blank line is not a diagnosis.
    let mut lines = stderr.lines().map(str::trim).filter(|line| !line.is_empty());
    if let Some(first) = lines.next() {
        let last = lines.next_back().unwrap_or(first);
        let Some(tool) = before_trailer(last) else {
            return last.to_string();
        };
        let mut kept = std::iter::once(first).chain(lines);
        // corner: a member error whose own name starts like that prefix reads as a tool line, and the archive-level rule below then judges it.
        let member = kept.clone().find(|line| tool.is_empty() || !line.starts_with(tool));
        // An archive-level error carries that prefix too, so only the warnings and retry notes beside it are passed over.
        let archive = kept.find(|line| line.strip_prefix(tool).is_some_and(|text| !tool_note(text)));
        return member.or(archive).unwrap_or(first).to_string();
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

// fontconfig's complaint when the jail has no writable cache: the head line, then one tab-indented line per directory.
const FONTCONFIG: &str = "Fontconfig ";
// SIGKILL is never the converter's own doing (the cancel or the kernel), so what it last printed is not why it stopped.
const SIGKILL: i32 = 9;

// Sample: status 137 with stderr "Fontconfig error: No writable cache directories\n\t/var/cache/fontconfig\n\t/.cache/fontconfig\n" gives the SIGKILL sentence.
// A SIGKILL is named by its signal; otherwise fontconfig's lines are no diagnosis and the rest is read as failure_message reads it.
pub(crate) fn office_failure_message(status: &ExitStatus, stderr: &str) -> String {
    let what = "office";
    let signal = status.signal().or_else(|| status.code().map(|code| code - BWRAP_SIGNAL_BASE));
    if signal == Some(SIGKILL) {
        return failure_message(what, status, "");
    }
    let mut in_fontconfig = false;
    let kept: Vec<&str> = stderr
        .lines()
        .filter(|line| {
            in_fontconfig = line.starts_with(FONTCONFIG) || (in_fontconfig && line.starts_with('\t'));
            !in_fontconfig
        })
        .collect();
    failure_message(what, status, &kept.join("\n"))
}
