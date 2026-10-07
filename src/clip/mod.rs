// The system clipboard for files, backend half: a std-only Wayland data-control client.
pub mod wire;
pub(crate) mod protocol;
mod receive;
mod end;
pub mod format;
pub mod control;
pub mod owner;
pub mod reply;
pub mod watch;
pub mod own;
pub mod cli;
pub mod x11;

// An explicit X11 session wins even if a stale WAYLAND_DISPLAY was inherited.
pub fn use_x11() -> bool {
    std::env::var_os("DISPLAY").is_some()
        && (std::env::var("XDG_SESSION_TYPE").ok().as_deref() == Some("x11")
            || std::env::var_os("WAYLAND_DISPLAY").is_none())
}

#[cfg(test)]
mod testutil;

// Serialises the tests that borrow process-global Wayland names for a fake socket.
#[cfg(test)]
pub(crate) static ENV_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

// Holds ENV_GUARD while WAYLAND_DISPLAY names a scratch socket (or nothing), restoring the old value on drop.
#[cfg(test)]
pub(crate) struct DisplayEnv {
    _held: std::sync::MutexGuard<'static, ()>,
    old: Option<std::ffi::OsString>,
}

#[cfg(test)]
impl DisplayEnv {
    pub(crate) fn set(socket: Option<&std::path::Path>) -> DisplayEnv {
        let held = ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var_os("WAYLAND_DISPLAY");
        match socket {
            Some(path) => std::env::set_var("WAYLAND_DISPLAY", path),
            None => std::env::remove_var("WAYLAND_DISPLAY"),
        }
        DisplayEnv { _held: held, old }
    }
}

#[cfg(test)]
impl Drop for DisplayEnv {
    fn drop(&mut self) {
        match self.old.take() {
            Some(old) => std::env::set_var("WAYLAND_DISPLAY", old),
            None => std::env::remove_var("WAYLAND_DISPLAY"),
        }
    }
}
