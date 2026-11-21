//! Wakes the presentation worker as soon as a folder holding one of its inputs changes; see `crate::dirwake`.
use std::{path::PathBuf, sync::mpsc::Sender, time::Duration};

pub(super) enum Signal {
    Wake,
    Stop,
}

/// Sends a wake per change; the per-`tick` wake remains only while inotify cannot watch every folder.
pub(super) fn start(
    dirs: Vec<PathBuf>,
    tick: Duration,
    wake: Sender<Signal>,
) -> crate::dirwake::DirWake {
    crate::dirwake::start("presentation", dirs, tick, move || {
        wake.send(Signal::Wake).is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::{start, Signal};
    use crate::presentation::{fixture::Fixture, source::Sources};
    use std::{sync::mpsc, time::Duration};

    // A tick no test run reaches, so only an inotify event can send the wake.
    const UNREACHED_TICK: Duration = Duration::from_secs(3600);
    // Turns a dead inotify path into a failure instead of a hang.
    const GUARD: Duration = Duration::from_secs(10);

    // Omarchy swaps the theme folder by renaming it into place; that rename alone must wake the worker (THEME-GLASS, native2 GPUI 475 ms, v0.3.12 27 ms).
    #[test]
    fn theme_glass_a_theme_folder_rename_wakes_the_worker_without_a_tick() {
        let fixture = Fixture::new("wake");
        let dir = fixture.path().to_path_buf();
        let current = dir.join("current");
        let theme = current.join("theme");
        let inputs = [
            theme.join("colors.toml"),
            theme.join("shell.toml"),
            dir.join("config/shell.toml"),
            dir.join("fontconfig/fonts.conf"),
            dir.join("fontconfig/conf.d/im.conf"),
            dir.join("toggles/window-opaque.lua"),
            dir.join("toggles/window-glassy.lua"),
        ];
        fixture.mkdir(&current.join("next"));
        let (wake, woken) = mpsc::channel();
        let _wake = start(
            Sources::at(inputs, dir.join("fc-match")).dirs(),
            UNREACHED_TICK,
            wake,
        );
        std::fs::rename(current.join("next"), &theme).unwrap();
        assert!(
            matches!(woken.recv_timeout(GUARD), Ok(Signal::Wake)),
            "renaming the theme folder into a watched folder must wake the worker"
        );
        drop(woken);
        fixture.cleanup();
    }
}
