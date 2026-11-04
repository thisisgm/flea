//! Flea owns crash recovery; Quickshell must leave a fatal signal visible instead of execing another window.
use std::ffi::OsStr;
use std::process::Command;

const DISABLE: &str = "QS_DISABLE_CRASH_HANDLER";
const MARKER: &str = "FLEA_QS_CRASH_POLICY";

fn ours(disabled: Option<&OsStr>, marker: Option<&OsStr>) -> bool {
    disabled == Some(OsStr::new("1")) && marker == Some(OsStr::new("1"))
}

/// Applied before qs starts: its crash handler initializes before root QML environment pragmas.
pub fn disable(command: &mut Command) {
    let disabled = std::env::var_os(DISABLE);
    let marker = std::env::var_os(MARKER);
    command.env_remove(MARKER);
    // Quickshell tests presence, so even an operator's empty value already disables its handler.
    if disabled.is_none() || ours(disabled.as_deref(), marker.as_deref()) {
        command.env(DISABLE, "1").env(MARKER, "1");
    }
}

/// An opened program or the GPUI frontend keeps the operator's policy and drops only Flea's pair.
pub fn restore(command: &mut Command) {
    if ours(
        std::env::var_os(DISABLE).as_deref(),
        std::env::var_os(MARKER).as_deref(),
    ) {
        command.env_remove(DISABLE);
    }
    command.env_remove(MARKER);
}
