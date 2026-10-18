use crate::{chooser, defaults};

// --default owns both per-user steps, because a user updating from 0.1.3 has no picker routing yet.
pub(super) fn claim_both() -> i32 {
    let handler = defaults::claim();
    // A refused handler claim has claimed nothing, so the picker half must not write either.
    if handler != 0 {
        return handler;
    }
    // A source build has no flea.portal to prefer, which is the picker's precondition, not a failure here.
    let status = if chooser::backend_installed() {
        chooser::claim("flea --default")
    } else {
        eprintln!("flea: no file chooser service is installed, so Flea could not become the file chooser");
        0
    };
    // The one undo line this invocation ends on: --default off releases both halves it just claimed.
    println!("undo both with: flea --default off");
    status
}

// flea --picker on its own, so it owns the undo line --default must not print for it.
pub(super) fn claim_picker() -> i32 {
    let installed = chooser::backend_installed();
    let status = chooser::claim("flea --picker");
    if installed {
        println!("undo both with: flea --picker off");
    }
    status
}

pub(super) fn release_both() -> i32 {
    let handler = defaults::release();
    let picker = chooser::release();
    handler.max(picker)
}
