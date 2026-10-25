//! Agent, custom actions and plugin-contributed menu rows, shared by the window and the TUI: the backend owns discovery and running (`src/backend/actions.rs`), this module owns the frontend half (script listing, request lines, reply readers, Open in Agent row, Actions flyout rows, plugin menu-action rows), and every row comes out as an `mn1` entry spliced into the inventory order.
pub mod agent;
pub mod plugins;
pub mod rows;
pub mod scripts;
pub mod wire;

#[cfg(test)]
mod rows_tests;
#[cfg(test)]
mod scripts_tests;
#[cfg(test)]
mod wire_tests;
