pub mod backend;
mod bench_thumbs;
mod capture;
#[cfg(test)]
#[path = "../../../tests/lib/cargo_artifacts.rs"]
mod cargo_artifacts;
pub mod columns;
pub(crate) mod compositor;
#[cfg(test)]
mod compositor_surface_tests;
pub mod driver_notice;
pub mod dual;
pub mod feedback;
#[cfg(test)]
mod font_gate;
pub mod g2;
pub mod grid;
pub mod input;
pub mod ipc;
pub mod leaf;
pub mod list;
pub mod locale_geometry;
pub(crate) mod name_tip;
pub mod navigation;
pub mod palette;
pub mod pane;
pub mod path;
pub mod presentation;
pub(crate) mod preview_quality;
pub(crate) mod preview_trace;
pub mod rail;
mod reveal;
pub mod row;
pub mod state;
pub mod strips;
pub mod tabs;
mod visual;
pub mod window;

#[cfg(test)]
#[path = "../../../src/i18n/test_catalogs.rs"]
mod test_catalogs;
// test_catalogs reaches the catalog types through super.
#[cfg(test)]
use flea::i18n::{I18n, Paths};

#[cfg(test)]
mod theme_gate;

#[cfg(test)]
mod bfx_cols_tests;
