// The open mount went away: the pane leaves the volume for its nearest parent; see AGENTS.md "A list scans off the event loop, so a slow mount cannot freeze progress or cancel".
use crate::backend::run::say;
use crate::backend::watch::{changed_line, nearest_parent, unmounted_line, Watch};
use crate::backend::watchpoll::Poller;
use std::io::Write;
use std::path::Path;

// Only the listing's own descriptors count, so the loop asks after a scan lands and not while one is out.
pub fn leave_mount(out: &mut impl Write, base: &Path, watch: &Watch, poller: &Poller, wd: i32) {
    if watch.is_open_folder(wd) {
        let parent = nearest_parent(&watch.unmount_start(base), |p| p.symlink_metadata().is_ok());
        say(out, &unmounted_line(base, &parent));
        poller.clear();
    } else if watch.is_current(wd) {
        // An expanded child's volume went, so the open folder stays and re-lists without it.
        say(out, &changed_line(base));
    }
}
