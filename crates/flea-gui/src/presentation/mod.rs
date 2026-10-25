//! Immutable presentation inputs, resolved off the UI thread and installed in one update.
mod face;
mod face_cache;
#[cfg(test)]
pub(crate) mod fixture;
pub(crate) mod fonts;
#[cfg(test)]
mod regression_tests;
mod service;
mod source;
pub use fonts::Fonts;
pub use service::Service;
pub use source::Snapshot;
