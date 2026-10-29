//! Resolve the operable cursor file before opening it.
use super::*;

// The cursor row's path and whether Quick Look owns it, when the pane is idle and the row is a loaded file.
pub(super) fn file_row(shell: &Shell, side: usize) -> Option<(String, bool)> {
    let host = shell.panes[side].as_ref()?;
    if host.pane.busy()
        || host.stopped
        || !host
            .search
            .owner
            .operable(host.pane.held.listing, host.pane.cursor)
    {
        return None;
    }
    let row = host.pane.held.row(host.pane.cursor)?;
    if row.dir || row.name.is_empty() {
        return None;
    }
    // Nav.js:239: an AppImage is a program first, so it asks on the run card instead.
    let archive = quick_look_kind(&row.icon, &row.name, false) == State::Archive
        && !row.name.to_ascii_lowercase().ends_with(".appimage");
    Some((join(host.pane.path(), &row.name), archive))
}

pub(super) fn archive_path(path: &str) -> bool {
    quick_look_kind("", path, false) == State::Archive
        && !path.to_ascii_lowercase().ends_with(".appimage")
}
