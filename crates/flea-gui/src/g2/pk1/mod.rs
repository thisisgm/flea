//! Picker portal and lifetime: the request, its one answer, the reply file and both read-only children.
mod answer;
pub mod browse;
pub mod filename;
mod find;
pub mod ipc;
pub mod labels;
pub mod launch;
pub mod picker;
pub mod process;
pub mod reply;
mod route;
mod view;

pub use picker::{Children, ListingFacts, Message, Picker};

#[cfg(test)]
mod tests;
