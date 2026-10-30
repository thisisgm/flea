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
pub mod content;

#[cfg(test)]
pub(crate) mod testutil;
