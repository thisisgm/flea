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

// Holds ENV_GUARD while Wayland tests set a scratch socket, without selecting a host X11 display.
#[cfg(test)]
pub(crate) struct DisplayEnv {
    _held: std::sync::MutexGuard<'static, ()>,
    old_wayland: Option<std::ffi::OsString>,
    old_display: Option<std::ffi::OsString>,
    old_session_type: Option<std::ffi::OsString>,
}

#[cfg(test)]
impl DisplayEnv {
    pub(crate) fn set(socket: Option<&std::path::Path>) -> DisplayEnv {
        let held = ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let old_wayland = std::env::var_os("WAYLAND_DISPLAY");
        let old_display = std::env::var_os("DISPLAY");
        let old_session_type = std::env::var_os("XDG_SESSION_TYPE");
        std::env::remove_var("DISPLAY");
        std::env::remove_var("XDG_SESSION_TYPE");
        match socket {
            Some(path) => std::env::set_var("WAYLAND_DISPLAY", path),
            None => std::env::remove_var("WAYLAND_DISPLAY"),
        }
        DisplayEnv { _held: held, old_wayland, old_display, old_session_type }
    }
}

#[cfg(test)]
impl Drop for DisplayEnv {
    fn drop(&mut self) {
        match self.old_display.take() {
            Some(old) => std::env::set_var("DISPLAY", old),
            None => std::env::remove_var("DISPLAY"),
        }
        match self.old_session_type.take() {
            Some(old) => std::env::set_var("XDG_SESSION_TYPE", old),
            None => std::env::remove_var("XDG_SESSION_TYPE"),
        }
        match self.old_wayland.take() {
            Some(old) => std::env::set_var("WAYLAND_DISPLAY", old),
            None => std::env::remove_var("WAYLAND_DISPLAY"),
        }
    }
}
