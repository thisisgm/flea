//! Immutable presentation inputs, resolved off the UI thread and installed in one update.
pub mod epoch;
#[cfg(test)]
mod epoch_tests;
mod face;
mod face_cache;
#[cfg(test)]
pub(crate) mod fixture;
pub(crate) mod fonts;
#[cfg(test)]
mod frame_census;
#[cfg(test)]
pub(crate) mod frame_gate;
#[cfg(test)]
mod frame_gate_tests;
pub mod frames;
#[cfg(test)]
mod regression_tests;
mod service;
mod source;
#[cfg(test)]
mod trash_font_tests;
mod wake;
pub use fonts::Fonts;
pub use service::Service;
#[cfg(test)]
pub(crate) use service::RETRY_PUBLISH;
pub use source::Snapshot;
