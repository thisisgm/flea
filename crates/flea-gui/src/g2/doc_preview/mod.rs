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
const LINE_PITCH: f32 = 1.4;
/// The CSV table's compact pitch, as a multiple of the caption size.
const TABLE_LINE_PITCH: f32 = 1.3;

/// The board's line box from its presentation size: 17 px for a 12 px caption, 20 px for a 14 px body.
pub fn line_pitch(caption: f32) -> f32 {
    (caption * LINE_PITCH).round()
}

/// CSV retains its compact caption line box.
pub fn table_line_pitch(caption: f32) -> f32 {
    (caption * TABLE_LINE_PITCH).round()
}

// Office documents and the CSV table view.
pub mod csv;
pub mod office;

#[cfg(test)]
mod scroll_tests;

#[test]
fn vf4_code_pitch_follows_the_caption_token_at_each_text_size() {
    for (caption, expected) in [(10., 14.), (12., 17.), (14., 20.), (16., 22.)] {
        assert_eq!(line_pitch(caption), expected);
    }
    assert_eq!(
        table_line_pitch(12.),
        16.,
        "CSV keeps its 16 px rows at a 12 px caption"
    );
}
