//! Documents in the preview: the PDF surface, rendered in process with a page cache, for the column's stack and Quick Look.
mod cache;
pub mod markdown;
mod paging;
pub mod poppler;
pub mod render;
pub mod selection;
pub mod surface;
pub mod view;
// The web part: the code view, HTML, font and archive pages.
pub mod archive;
pub mod code;
pub mod font;
pub mod html;

pub use render::reap;
pub use view::{Bar, CopyHost, Metrics, Palette, PdfView};

/// The views' line pitch, as a multiple of the caption size.
const LINE_PITCH: f32 = 1.3;

/// A caption's line pitch, which the code rows and the table rows take: 16 px at a 12 px caption.
pub fn line_pitch(caption: f32) -> f32 {
    (caption * LINE_PITCH).round()
}

// Office documents and the CSV table view.
pub mod csv;
pub mod office;

#[cfg(test)]
mod scroll_tests;
