//! Documents in the preview: the PDF surface, rendered in process with a page cache, for the column's stack and Quick Look.
mod cache;
pub mod markdown;
pub mod poppler;
pub mod render;
pub mod surface;
pub mod view;
// The web part: the code view, HTML, font and archive pages.
pub mod archive;
pub mod code;
pub mod font;
pub mod html;

pub use view::{Bar, CopyHost, Metrics, Palette, PdfView};
