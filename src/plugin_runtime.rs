//! The executable plugin protocol and supervised, bounded sessions shared by Flea hosts.
mod process;
pub mod protocol;
mod session;
pub use session::{Config, Reply, Runtime};

#[cfg(test)]
mod tests;
