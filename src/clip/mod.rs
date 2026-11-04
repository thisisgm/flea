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
pub(crate) mod testutil;
