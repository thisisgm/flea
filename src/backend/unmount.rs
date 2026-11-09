// The open mount went away: the pane leaves the volume for its nearest parent; see AGENTS.md "A list scans off the event loop, so a slow mount cannot freeze progress or cancel".
use crate::backend::iomount::{ListErr, ListOut};
use crate::backend::run::say;
use crate::backend::state::{Held, State};
use crate::backend::watch::ancestry::Gone;
use crate::backend::watch::{changed_line, gone_line, nearest_parent, unmounted_line, Watch};
use crate::backend::watchpoll::Poller;
use std::io::Write;
use std::path::Path;

// Only the listing's own descriptors count, so the loop asks after a scan lands and not while one is out.
pub fn leave_mount(out: &mut impl Write, base: &Path, watch: &mut Watch, poller: &Poller, wd: i32) {
    if watch.is_open_folder(wd) {
        let parent = nearest_parent(&watch.unmount_start(base), |p| p.symlink_metadata().is_ok());
        say(out, &unmounted_line(base, &parent));
        poller.clear();
        // The volume's own line moved the pane, so its folder vanishing with it is not said again.
        watch.ancestry.forget();
    } else if watch.is_current(wd) {
        // An expanded child's volume went, so the open folder stays and re-lists without it.
        say(out, &changed_line(base));
    }
}

// The open folder, or the one the last gone line sent the client to, is no longer there; a walk or a path list follows nothing.
pub fn leave_gone(
    out: &mut impl Write,
    st: &State,
    watch: &mut Watch,
    poller: &Poller,
    gone: Gone,
) {
    if st.held != Held::List || !watch.ancestry.owes(&gone.path, &st.base) {
        return;
    }
    say(out, &gone_line(&gone.path, &gone.parent));
    watch.ancestry.left_for(&gone);
    poller.clear();
}

// A re-list that found its folder gone answers gone instead of an error, so the pane never draws a folder that could not be read on its way out.
pub fn gone_answers(
    out: &mut impl Write,
    st: &State,
    watch: &mut Watch,
    poller: &Poller,
    path: &Path,
    failed: &ListErr,
) -> bool {
    // A folder replaced by a file fails as "not a folder", and the follower already calls that gone.
    const ENOTDIR: i32 = 20;
    let missing = crate::error::io_message(&std::io::Error::from(std::io::ErrorKind::NotFound));
    let not_folder = crate::error::io_message(&std::io::Error::from_raw_os_error(ENOTDIR));
    if (failed.error.msg != missing && failed.error.msg != not_folder) || st.held != Held::List {
        return false;
    }
    if !watch.ancestry.owes(path, &st.base) {
        return false;
    }
    let parent = nearest_parent(path, |p| p.is_dir());
    leave_gone(
        out,
        st,
        watch,
        poller,
        Gone {
            path: path.to_path_buf(),
            parent,
        },
    );
    true
}

// A list the client sent before it read the gone line, inside the folder that went, neither lands nor fails: the line is said again and its watches go.
pub fn crossed_answers(
    out: &mut impl Write,
    watch: &mut Watch,
    path: &Path,
    result: &Result<ListOut, ListErr>,
) -> bool {
    let Some(gone) = watch.ancestry.crossed(path).cloned() else {
        return false;
    };
    if let Ok(listed) = result {
        for &wd in &listed.listing.outline_watches {
            watch.abandon_wd(wd);
        }
    }
    watch.abandon_wd(match result {
        Ok(listed) => listed.watch_wd,
        Err(failed) => failed.watch_wd,
    });
    say(out, &gone_line(&gone.path, &gone.parent));
    true
}
