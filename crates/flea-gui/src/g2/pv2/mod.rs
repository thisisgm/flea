//! Documents in the preview: the PDF surface, rendered in process with a page cache, for the column's stack and Quick Look.
mod cache;
pub mod poppler;
pub mod render;
pub mod surface;
pub mod view;

pub use view::{Bar, CopyHost, Metrics, Palette, PdfView};
