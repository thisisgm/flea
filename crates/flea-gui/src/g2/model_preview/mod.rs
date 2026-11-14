//! 3D model and toolpath previews behind the preview host's seam: STL, 3MF, OBJ, PLY, AMF, G-code and binary G-code; the host mounts a `Model3dView` per slot, forwards backend lines and key actions, and draws its bar and facts with `parts`.
pub mod element;
#[cfg(test)]
mod frame_tests;
pub mod live;
#[cfg(test)]
mod m9_g2rows_16_tests;
#[cfg(test)]
mod memory_tests;
pub mod parts;
pub mod seam;
pub mod session;
#[cfg(test)]
mod tests;
pub mod view;

pub use seam::{Host, ModelReader, NoReader, Wire};
pub use view::{Bar, Model3dView, Phase};
