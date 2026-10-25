pub mod backend;
mod capture;
pub mod columns;
pub(crate) mod compositor;
#[cfg(test)]
mod compositor_surface_tests;
pub mod dual;
pub mod feedback;
pub mod g2;
pub mod grid;
pub mod input;
pub mod ipc;
pub mod list;
pub mod locale_geometry;
pub mod navigation;
pub mod palette;
pub mod pane;
pub mod path;
pub mod presentation;
pub mod rail;
pub mod row;
pub mod state;
pub mod strips;
pub mod tabs;
mod visual;
pub mod window;

#[cfg(test)]
mod theme_gate;
